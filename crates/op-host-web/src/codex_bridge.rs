//! Same-document adapter for Codex MCP Apps. The existing bridge codec and
//! sync gate still own document operations; no synthetic parent messages.

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use crate::live_sync_glue::SharedSync;
use crate::repaint_ctx::RepaintContext;
use op_editor_core::bridge_protocol::BridgeInbound;

#[derive(Debug, PartialEq)]
enum HostMessage {
    Editor(BridgeInbound),
    DisplayMode { fullscreen: bool },
    FitContent,
    ToggleSidebar,
    ToggleInspector,
    TogglePreview,
    SelectTool(op_editor_core::Tool),
    Zoom(bool),
    Toolbar(op_editor_ui::widgets::ToolbarAction),
    LayerSearch(String),
    LayersView(op_editor_core::LeftPanelTab),
    InspectorView(op_editor_core::PropertyTab),
}

impl HostMessage {
    fn parse(raw: &str) -> Option<Self> {
        if let Some(message) = BridgeInbound::parse(raw) {
            return Some(Self::Editor(message));
        }
        let value: serde_json::Value = serde_json::from_str(raw).ok()?;
        match value.get("type")?.as_str()? {
            "op-bridge/layer-search" => {
                let query = value.get("query")?.as_str()?.trim();
                return (query.chars().count() <= 100)
                    .then(|| Self::LayerSearch(query.to_lowercase()));
            }
            "op-bridge/layers-view" => {
                return match value.get("tab")?.as_str()? {
                    "layers" => Some(Self::LayersView(op_editor_core::LeftPanelTab::Layers)),
                    "slides" => Some(Self::LayersView(op_editor_core::LeftPanelTab::Slides)),
                    _ => None,
                }
            }
            "op-bridge/inspector-view" => {
                return match value.get("tab")?.as_str()? {
                    "design" => Some(Self::InspectorView(op_editor_core::PropertyTab::Design)),
                    "code" => Some(Self::InspectorView(op_editor_core::PropertyTab::Code)),
                    _ => None,
                }
            }
            "op-bridge/tool" => {
                let ident = value.get("tool")?.as_str()?;
                return op_editor_core::Tool::ALL
                    .into_iter()
                    .find(|tool| tool.ident() == ident)
                    .map(Self::SelectTool);
            }
            "op-bridge/zoom" => return Some(Self::Zoom(value.get("in")?.as_bool()?)),
            "op-bridge/editor-action" => {
                use op_editor_ui::widgets::ToolbarAction::*;
                let action = match value.get("action")?.as_str()? {
                    "undo" => Undo,
                    "redo" => Redo,
                    "variables" => ToggleVariablesPanel,
                    "resources" => ToggleDesignPanel,
                    _ => return None,
                };
                return Some(Self::Toolbar(action));
            }
            "op-bridge/toggle-inspector" => return Some(Self::ToggleInspector),
            "op-bridge/toggle-sidebar" => return Some(Self::ToggleSidebar),
            "op-bridge/toggle-preview" => return Some(Self::TogglePreview),
            _ => {}
        }
        if value.get("type")?.as_str()? == "op-bridge/fit-content" {
            return Some(Self::FitContent);
        }
        if value.get("type")?.as_str()? != "op-bridge/display-mode" {
            return None;
        }
        let fullscreen = match value.get("mode")?.as_str()? {
            "inline" => false,
            "fullscreen" => true,
            _ => return None,
        };
        Some(Self::DisplayMode { fullscreen })
    }
}

type MessageHandler = Rc<dyn Fn(HostMessage)>;
type SelectionHandler = Rc<dyn Fn() -> String>;
type NameHandler = Rc<dyn Fn(String)>;
thread_local! {
    static OUTBOUND: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static INBOUND: RefCell<Option<MessageHandler>> = const { RefCell::new(None) };
    static SELECTION: RefCell<Option<SelectionHandler>> = const { RefCell::new(None) };
    static NAME: RefCell<Option<NameHandler>> = const { RefCell::new(None) };
    static LAST_PANELS: RefCell<Option<(bool, bool, bool, bool, bool, usize)>> = const { RefCell::new(None) };
    static LAST_DOCK: RefCell<Option<(op_editor_core::Tool, i32)>> = const { RefCell::new(None) };
    static LAST_PREVIEW: RefCell<Option<bool>> = const { RefCell::new(None) };
    static EARLY: RefCell<Vec<HostMessage>> = const { RefCell::new(Vec::new()) };
}

/// Host DOM controls own their keyboard and clipboard events.
pub(crate) fn dom_owns_event(event: &web_sys::Event) -> bool {
    event.composed_path().iter().any(|node| {
        node.dyn_ref::<web_sys::Element>()
            .is_some_and(|element| element.has_attribute("data-openbrush-ui"))
    })
}

pub(crate) fn active() -> bool {
    web_sys::window()
        .and_then(|window| window.location().search().ok())
        .map(|query| {
            op_editor_core::EmbedHost::from_query(&query) == op_editor_core::EmbedHost::Codex
        })
        .unwrap_or(false)
}

#[wasm_bindgen]
pub fn codex_on_event(callback: js_sys::Function) -> Result<(), JsValue> {
    if !active() {
        return Err(JsValue::from_str("Codex embedding mode required"));
    }
    OUTBOUND.with(|slot| *slot.borrow_mut() = Some(callback));
    LAST_PANELS.with(|slot| *slot.borrow_mut() = None);
    LAST_PREVIEW.with(|slot| *slot.borrow_mut() = None);
    LAST_DOCK.with(|slot| *slot.borrow_mut() = None);
    Ok(())
}

#[wasm_bindgen]
pub fn codex_host_message(json: String) -> Result<(), JsValue> {
    if !active() || json.len() > 4 * 1024 * 1024 {
        return Err(JsValue::from_str("Invalid Codex editor message"));
    }
    let message =
        HostMessage::parse(&json).ok_or_else(|| JsValue::from_str("Unknown editor message"))?;
    let handler = INBOUND.with(|slot| slot.borrow().clone());
    if let Some(handler) = handler {
        handler(message);
    } else {
        EARLY.with(|queue| {
            let mut queue = queue.borrow_mut();
            if queue.len() >= 8 {
                return Err(JsValue::from_str("Editor is not ready"));
            }
            queue.push(message);
            Ok(())
        })?;
    }
    Ok(())
}

#[wasm_bindgen]
pub fn codex_document_name(name: String) -> Result<(), JsValue> {
    if !active() || name.len() > 512 || name.contains(['\n', '\r', '\0']) {
        return Err(JsValue::from_str("Invalid document name"));
    }
    let handler = NAME
        .with(|slot| slot.borrow().clone())
        .ok_or_else(|| JsValue::from_str("Editor is not ready"))?;
    handler(name);
    Ok(())
}

#[wasm_bindgen]
pub fn codex_selection_json() -> Result<String, JsValue> {
    if !active() {
        return Err(JsValue::from_str("Codex embedding mode required"));
    }
    let handler = SELECTION
        .with(|slot| slot.borrow().clone())
        .ok_or_else(|| JsValue::from_str("Editor is not ready"))?;
    Ok(handler())
}

pub(crate) fn document_replaced() {
    #[cfg(target_arch = "wasm32")]
    if active() {
        emit(r#"{"type":"op-shell/document-replaced"}"#);
    }
}

pub(crate) fn panels_state(state: &op_editor_core::EditorState) {
    if !active() {
        return;
    }
    let dock = (state.tool, (state.viewport.zoom * 100.0).round() as i32);
    LAST_DOCK.with(|slot| {
        if *slot.borrow() == Some(dock) {
            return;
        }
        *slot.borrow_mut() = Some(dock);
        emit(&format!(
            r#"{{"type":"op-shell/dock-state","tool":"{}","zoom":{}}}"#,
            dock.0.ident(),
            dock.1
        ));
    });
    let next = (
        state.editor_ui.sidebar_open,
        state.editor_ui.inspector_open,
        state.right_rail_visible(),
        op_editor_ui::widgets::host_canvas_geometry::codex_inspector_header(state),
        op_editor_ui::widgets::slides_panel_flow::slides_tab_active(state),
        state.page_count().min(3),
    );
    LAST_PANELS.with(|slot| {
        if *slot.borrow()==Some(next) { return; }
        *slot.borrow_mut()=Some(next);
        emit(&format!(r#"{{"type":"op-shell/panels-state","layers":{},"inspector":{},"inspectorVisible":{},"inspectorHeader":{},"slides":{},"layerPages":{}}}"#,next.0,next.1,next.2,next.3,next.4,next.5));
    });
}

pub(crate) fn preview_state(active: bool) {
    if !self::active() {
        return;
    }
    LAST_PREVIEW.with(|slot| {
        if *slot.borrow() == Some(active) {
            return;
        }
        *slot.borrow_mut() = Some(active);
        emit(&format!(
            r#"{{"type":"op-shell/preview-state","active":{active}}}"#
        ));
    });
}

pub(crate) fn emit(json: &str) {
    let callback = OUTBOUND.with(|slot| slot.borrow().clone());
    if let Some(callback) = callback {
        let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(json));
    }
}

pub(crate) fn install<C: RepaintContext + 'static>(inner: &Rc<RefCell<C>>, sync: SharedSync) {
    if !active() {
        return;
    }
    let selection_inner = inner.clone();
    SELECTION.with(|slot| {
        *slot.borrow_mut() = Some(Rc::new(move || {
            op_editor_core::web_sync::selection_push_body(
                selection_inner.borrow().host().editor_state(),
            )
        }))
    });
    let message_inner = inner.clone();
    let handler: MessageHandler = Rc::new(move |message| match message {
        HostMessage::Editor(message) => {
            crate::vscode_bridge::route_message(&message_inner, &sync, "codex-mcp-app", message);
        }
        HostMessage::DisplayMode { fullscreen } => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                let ui = &mut context.host_mut().editor_state_mut().editor_ui;
                if ui.window_fullscreen != fullscreen {
                    ui.window_fullscreen = fullscreen;
                    context.host_mut().mark_editor_state_dirty();
                    let _ = context.repaint();
                }
            }
        }
        HostMessage::ToggleSidebar => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                context.host_mut().editor_state_mut().editor_ui.sidebar_open ^= true;
                context.host_mut().mark_editor_state_dirty();
                let _ = context.repaint();
            }
        }
        HostMessage::ToggleInspector => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                context
                    .host_mut()
                    .editor_state_mut()
                    .editor_ui
                    .inspector_open ^= true;
                context.host_mut().mark_editor_state_dirty();
                let _ = context.repaint();
            }
        }
        HostMessage::TogglePreview => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                let (width, height) = context.viewport_size();
                context.host_mut().toggle_host_preview(width, height);
                let _ = context.repaint();
            }
        }
        HostMessage::SelectTool(tool) => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                context.host_mut().commit_codex_inputs();
                context.host_mut().apply_set_tool(tool);
                let _ = context.repaint();
            }
        }
        HostMessage::Zoom(zoom_in) => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                let (width, height) = context.viewport_size();
                context.host_mut().status_bar_zoom(zoom_in, width, height);
                let _ = context.repaint();
            }
        }
        HostMessage::Toolbar(action) => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                context.host_mut().commit_codex_inputs();
                context.host_mut().dispatch_toolbar_action(action);
                let _ = context.repaint();
            }
        }
        HostMessage::LayerSearch(query) => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                let host = context.host_mut();
                let ui = &mut host.editor_state_mut().editor_ui;
                if ui.layer_search_query != query {
                    ui.layer_search_query = query;
                    ui.layer_layers_scroll = Default::default();
                    ui.layer_layers_h_scroll = Default::default();
                    host.mark_editor_state_dirty();
                    let _ = context.repaint();
                }
            }
        }
        HostMessage::LayersView(tab) => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                let host = context.host_mut();
                let state = host.editor_state_mut();
                state.editor_ui.sidebar_open = true;
                op_editor_ui::widgets::slides_panel_flow::select_tab(state, tab);
                host.mark_editor_state_dirty();
                let _ = context.repaint();
            }
        }
        HostMessage::InspectorView(tab) => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                let host = context.host_mut();
                host.commit_codex_inputs();
                let ui = &mut host.editor_state_mut().editor_ui;
                ui.inspector_open = true;
                ui.set_property_tab(tab);
                host.mark_editor_state_dirty();
                let _ = context.repaint();
            }
        }
        HostMessage::FitContent => {
            if let Ok(mut context) = message_inner.try_borrow_mut() {
                let (width, height) = context.viewport_size();
                context.host_mut().fit_content_to_viewport(width, height);
                let _ = context.repaint();
            }
        }
    });
    INBOUND.with(|slot| *slot.borrow_mut() = Some(handler.clone()));
    let name_inner = inner.clone();
    NAME.with(|slot| {
        *slot.borrow_mut() = Some(Rc::new(move |name| {
            if let Ok(mut context) = name_inner.try_borrow_mut() {
                context
                    .host_mut()
                    .editor_state_mut()
                    .editor_ui
                    .file_name_display = Some(name);
                context.host_mut().mark_editor_state_dirty();
                let _ = context.repaint();
            }
        }))
    });
    for message in EARLY.with(|queue| std::mem::take(&mut *queue.borrow_mut())) {
        handler(message);
    }
}

#[cfg(test)]
mod tests {
    use super::HostMessage;
    use op_editor_core::bridge_protocol::BridgeInbound;

    #[test]
    fn display_mode_accepts_only_the_modes_exposed_by_the_app() {
        for (mode, fullscreen) in [("inline", false), ("fullscreen", true)] {
            let raw = format!(r#"{{"type":"op-bridge/display-mode","mode":"{mode}"}}"#);
            assert_eq!(
                HostMessage::parse(&raw),
                Some(HostMessage::DisplayMode { fullscreen })
            );
        }
        for raw in [
            r#"{"type":"op-bridge/display-mode","mode":"pip"}"#,
            r#"{"type":"op-bridge/display-mode","mode":"expanded"}"#,
            r#"{"type":"op-bridge/display-mode","mode":true}"#,
            r#"{"type":"op-bridge/display-mode"}"#,
            r#"{"type":"op-shell/display-mode","mode":"fullscreen"}"#,
            "not json",
        ] {
            assert_eq!(HostMessage::parse(raw), None, "{raw}");
        }
    }

    #[test]
    fn display_mode_extension_preserves_the_existing_editor_codec() {
        assert_eq!(
            HostMessage::parse(
                r#"{"type":"op-bridge/snapshot","purpose":"save","requestId":"r1"}"#
            ),
            Some(HostMessage::Editor(BridgeInbound::Snapshot {
                purpose: "save".to_owned(),
                request_id: "r1".to_owned(),
            }))
        );
    }
}
