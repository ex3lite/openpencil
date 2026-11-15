//! Same-document adapter for Codex MCP Apps. The existing bridge codec and
//! sync gate still own document operations; no synthetic parent messages.

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use crate::live_sync_glue::SharedSync;
use crate::repaint_ctx::RepaintContext;
use op_editor_core::bridge_protocol::BridgeInbound;

type MessageHandler = Rc<dyn Fn(BridgeInbound)>;
type SelectionHandler = Rc<dyn Fn() -> String>;
type NameHandler = Rc<dyn Fn(String)>;
thread_local! {
    static OUTBOUND: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static INBOUND: RefCell<Option<MessageHandler>> = const { RefCell::new(None) };
    static SELECTION: RefCell<Option<SelectionHandler>> = const { RefCell::new(None) };
    static NAME: RefCell<Option<NameHandler>> = const { RefCell::new(None) };
    static EARLY: RefCell<Vec<BridgeInbound>> = const { RefCell::new(Vec::new()) };
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
    Ok(())
}

#[wasm_bindgen]
pub fn codex_host_message(json: String) -> Result<(), JsValue> {
    if !active() || json.len() > 4 * 1024 * 1024 {
        return Err(JsValue::from_str("Invalid Codex editor message"));
    }
    let message =
        BridgeInbound::parse(&json).ok_or_else(|| JsValue::from_str("Unknown editor message"))?;
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
    let handler: MessageHandler = Rc::new(move |message| {
        crate::vscode_bridge::route_message(&message_inner, &sync, "codex-mcp-app", message);
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
