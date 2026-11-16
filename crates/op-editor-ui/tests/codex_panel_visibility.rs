use op_editor_core::{EditorState, NodeId};
use op_editor_ui::widgets::PropertyPanel;

#[test]
fn a_hidden_inspector_has_no_widget_or_hit_target() {
    let doc=serde_json::from_str(r#"{"version":"0.8.5","children":[{"id":"button","type":"frame","width":100,"height":40}]}"#).unwrap();
    let mut state = EditorState::from_document(doc);
    state.selection.set.push(NodeId::new("button"));
    state.selection.anchor = NodeId::new("button");
    assert!(PropertyPanel::for_selection(&state).is_some());
    state.editor_ui.inspector_open = false;
    assert!(PropertyPanel::for_selection(&state).is_none());
    assert_eq!(state.selection_count(), 1);
    state.editor_ui.inspector_open = true;
    assert!(PropertyPanel::for_selection(&state).is_some());
}
