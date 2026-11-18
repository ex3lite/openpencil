//! The authored inspector edits the same fields and history as the native panel.
use op_editor_core::{EditorState, PropertyFocus as F};
use op_editor_ui::widgets::{property_panel_commit, property_panel_layout_ops, PropertyPanel};
use serde_json::{json, Value};
use std::cell::RefCell;

const FIELDS: &[(&str, F)] = &[
    ("x", F::PositionX),
    ("y", F::PositionY),
    ("width", F::SizeW),
    ("height", F::SizeH),
    ("weight", F::FontWeight),
    ("size", F::FontSize),
    ("leading", F::LineHeight),
    ("tracking", F::LetterSpacing),
    ("fill", F::FillHex(0)),
    ("alpha", F::FillOpacity(0)),
];

#[derive(PartialEq)]
struct Key(u64, u64, usize, String);
thread_local! { static LAST: RefCell<Option<Key>> = const { RefCell::new(None) }; }

pub(crate) fn reset() {
    LAST.with(|key| key.borrow_mut().take());
}

fn text_panel(state: &EditorState) -> Option<PropertyPanel> {
    if state.selection_count() != 1
        || !op_editor_ui::widgets::host_canvas_geometry::codex_inspector_header(state)
        || !matches!(
            state.selected_node(),
            Some(jian_ops_schema::node::PenNode::Text(_))
        )
    {
        return None;
    }
    PropertyPanel::for_selection(state)
}

pub(crate) fn publish(state: &EditorState, visible: bool) {
    let id = if visible && state.selection_count() == 1 {
        state.selection.anchor.as_str()
    } else {
        ""
    };
    let key = Key(
        state.document_generation(),
        state.document_revision(),
        state.ui.active_page_index,
        id.to_owned(),
    );
    if LAST.with(|last| {
        if last.borrow().as_ref() == Some(&key) {
            return true;
        }
        last.replace(Some(key));
        false
    }) {
        return;
    }
    let fields = (!id.is_empty())
        .then(|| text_panel(state))
        .flatten()
        .and_then(|panel| {
            let mut values = serde_json::Map::new();
            for &(name, focus) in FIELDS {
                values.insert(
                    name.into(),
                    Value::String(property_panel_commit::property_focus_initial(focus, &panel)),
                );
            }
            values.insert(
                "family".into(),
                json!(panel.snapshot.text.as_ref()?.font_family),
            );
            let bindings: Vec<_> = panel
                .snapshot
                .fills
                .iter()
                .filter_map(|fill| fill.variable_ref.as_deref())
                .collect();
            values.insert("bindings".into(), json!(bindings));
            Some(values)
        });
    super::emit(
        &json!({"type":"op-shell/text-properties","id":id,"generation":state.document_generation(),
        "page":state.ui.active_page_index,"fields":fields})
        .to_string(),
    );
}

pub(crate) fn apply(state: &mut EditorState, message: &Value) -> bool {
    if message["generation"].as_u64() != Some(state.document_generation())
        || message["page"].as_u64() != Some(state.ui.active_page_index as u64)
        || message["id"].as_str() != Some(state.selection.anchor.as_str())
    {
        return false;
    }
    let Some(panel) = text_panel(state) else {
        return false;
    };
    let Some(value) = message["value"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
    else {
        return false;
    };
    if message["field"] == "family" {
        if panel.snapshot.text.as_ref().map(|t| t.font_family.as_str())
            != message["before"].as_str()
        {
            return false;
        }
        property_panel_layout_ops::set_selected_text_font_family(state, value);
        return true;
    }
    let Some(&(_, focus)) = FIELDS
        .iter()
        .find(|(name, _)| Some(*name) == message["field"].as_str())
    else {
        return false;
    };
    if message["before"].as_str()
        != Some(&property_panel_commit::property_focus_initial(
            focus, &panel,
        ))
    {
        return false;
    }
    if focus == F::FillHex(0) {
        let hex = value.trim_start_matches('#');
        if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
    } else if !value.parse::<f32>().is_ok_and(f32::is_finite) {
        return false;
    }
    state.ui.property_focus = Some(focus);
    state.ui.property_input.set_text(value);
    property_panel_commit::commit_property_focus(state)
}
