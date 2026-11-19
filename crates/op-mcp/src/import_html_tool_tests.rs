use std::collections::BTreeMap;

use super::import_html_tool::{import_html_for_document, import_html_snapshot, ImportHtml};
use super::{EditorCommand, McpTool, ToolOutcome};

#[test]
fn import_html_returns_insert_subtree_command() {
    let tool: ImportHtml = import_html_snapshot();
    let mut args = BTreeMap::new();
    args.insert(
        "html".to_string(),
        "<div style=\"display:flex\"><p>hi</p></div>".to_string(),
    );
    args.insert("x".to_string(), "100".to_string());
    let outcome = tool.call(&args);
    let ToolOutcome::OkWithCommand(result, EditorCommand::InsertSubtree { nodes, page_id, .. }) =
        outcome
    else {
        panic!("expected OkWithCommand(InsertSubtree)")
    };
    assert_eq!(result.get("wrote").map(String::as_str), Some("true"));
    assert_eq!(nodes.len(), 1);
    assert!(page_id.is_none());
}

#[test]
fn missing_html_is_typed_error() {
    let tool = import_html_snapshot();
    let outcome = tool.call(&BTreeMap::new());
    assert!(matches!(outcome, ToolOutcome::Err(..)));
}

#[test]
fn editable_html_import_remaps_component_and_override_and_survives_edit_save_reopen() {
    use jian_ops_schema::node::PenNode;
    use op_editor_core::{EditorState, NodeId, PenNodeExt};
    use serde_json::json;

    let model = json!({
        "version":"1.0",
        "variables":{"brand":{"type":"color","value":"#162738"}},
        "themes":{"mode":["light","dark"]},
        "children":[{"type":"frame","id":"panel","name":"Panel","layout":"vertical","children":[
            {"type":"text","id":"copy","content":"Before"},
            {"type":"ref","id":"button","ref":"master","descendants":{"label":{"content":"CTA"}}},
            {"type":"path","id":"icon","d":"M0 0L10 10","stroke":{"thickness":2,"fill":[{"type":"solid","color":"$brand"}]}}
        ]}],
        "pages":[{"id":"defs","name":"Components","children":[
            {"type":"frame","id":"master","reusable":true,"children":[{"type":"text","id":"label","content":"Button"}]}
        ]}]
    });
    let html = op_util::editable_html::embed_editable_document(
        "<body><p>Before</p></body>",
        &model.to_string(),
    );
    let args = BTreeMap::from([("html".to_string(), html)]);
    let mut state = EditorState::new();
    for _ in 0..2 {
        let ToolOutcome::OkWithCommand(_, cmd) = import_html_for_document(&state).call(&args)
        else {
            panic!("expected a write");
        };
        assert!(state.apply(cmd));
    }
    let roots = state.active_children();
    assert_eq!(roots.len(), 4);
    for pair in roots.chunks_exact(2) {
        let panel = pair[0].children_ref().unwrap();
        let master = &pair[1].children_ref().unwrap()[0];
        let PenNode::Ref(instance) = &panel[1] else {
            panic!()
        };
        assert_eq!(instance.target, master.id_str());
        assert!(instance
            .descendants
            .as_ref()
            .unwrap()
            .contains_key(master.children_ref().unwrap()[0].id_str()));
        assert!(
            matches!(&panel[2], PenNode::Path(path) if path.d.as_deref() == Some("M0 0L10 10"))
        );
        assert_eq!(pair[1].base().visible, Some(false));
    }
    let text_id = NodeId::new(roots[0].children_ref().unwrap()[0].id_str());
    assert!(state.apply(EditorCommand::SetNodeText {
        node_id: text_id.clone(),
        text: "After editing".into()
    }));
    let file = serde_json::to_vec(&state.doc).unwrap();
    let reopened = EditorState::from_document(serde_json::from_slice(&file).unwrap());
    let text = &reopened.active_children()[0].children_ref().unwrap()[0];
    assert_eq!(text.id_str(), text_id.as_str());
    assert_eq!(
        serde_json::to_value(text).unwrap()["content"],
        "After editing"
    );
    assert!(reopened
        .doc
        .variables
        .as_ref()
        .unwrap()
        .contains_key("brand"));
    assert_eq!(reopened.doc.themes.as_ref().unwrap()["mode"].len(), 2);
}

#[test]
fn roundtrip_preserves_nested_component_override_keys_and_swapped_targets() {
    use op_editor_core::{EditorState, PenNodeExt};
    use serde_json::{json, Value};

    let source: jian_ops_schema::PenDocument =
        serde_json::from_value(json!({"version":"1","children":[
            {"type":"ref","id":"screen","ref":"card","descendants":{
                "button":{"descendants":{
                    "icon":{"ref":"alt-icon","descendants":{
                        "alt-label":{"content":"Selected","events":{"onTap":[]}}
                    }}
                }}
            }},
            {"type":"frame","id":"card","reusable":true,"children":[
                {"type":"ref","id":"button","ref":"button-master"}
            ]},
            {"type":"frame","id":"button-master","reusable":true,"children":[
                {"type":"ref","id":"icon","ref":"default-icon"}
            ]},
            {"type":"frame","id":"default-icon","reusable":true},
            {"type":"frame","id":"alt-icon","name":"Swapped master","reusable":true,"children":[
                {"type":"text","id":"alt-label","content":"Default"}
            ]}
        ]}))
        .unwrap();
    let selection = &source.children[0];
    let html = op_codegen::Html.generate_selection(&source, [selection], &Default::default());
    let mut state = EditorState::new();
    let args = BTreeMap::from([("html".to_owned(), html)]);
    let ToolOutcome::OkWithCommand(_, cmd) = import_html_for_document(&state).call(&args) else {
        panic!("expected native import command")
    };
    assert!(state.apply(cmd));
    let roots = state.active_children();
    let masters = roots[1].children_ref().unwrap();
    let [card, button_master, _, swapped] = masters else {
        panic!("missing master dependency")
    };
    let nested_button = card.children_ref().unwrap()[0].id_str();
    let nested_icon = button_master.children_ref().unwrap()[0].id_str();
    let nested_label = swapped.children_ref().unwrap()[0].id_str();
    let instance = serde_json::to_value(&roots[0]).unwrap();
    let patch = &instance["descendants"][nested_button]["descendants"][nested_icon];
    assert_eq!(patch["ref"], swapped.id_str());
    assert_eq!(patch["descendants"][nested_label]["content"], "Selected");
    assert_eq!(patch["descendants"][nested_label]["events"], Value::Null);
}

#[test]
fn conflicting_token_in_existing_document_aborts_without_overwriting_it() {
    use op_editor_core::EditorState;
    use serde_json::json;
    let model = json!({"version":"1","variables":{"ink":{"type":"color","value":"#ff0000"}},
        "children":[{"type":"text","id":"headline","content":"Text"}]});
    let args = BTreeMap::from([(
        "html".into(),
        op_util::editable_html::embed_editable_document("<p>Text</p>", &model.to_string()),
    )]);
    let doc = serde_json::from_value(
        json!({"version":"1","variables":{"ink":{"type":"color","value":"#000000"}}}),
    )
    .unwrap();
    let state = EditorState::from_document(doc);
    assert!(
        matches!(import_html_for_document(&state).call(&args), ToolOutcome::Err(super::ToolErrorCode::InvalidArgument, message) if message.contains("ink"))
    );
}

#[test]
fn placing_multiple_selected_leaves_at_zero_preserves_relative_offsets() {
    use op_editor_core::PenNodeExt;
    let source = serde_json::json!({"version":"1","children":[
        {"type":"text","id":"a","x":100,"y":3,"content":"A"},
        {"type":"text","id":"b","x":124,"y":17,"content":"B"}]});
    let args = BTreeMap::from([
        (
            "html".into(),
            op_util::editable_html::embed_editable_document("<p>A B</p>", &source.to_string()),
        ),
        ("x".into(), "0".into()),
    ]);
    let ToolOutcome::OkWithCommand(_, EditorCommand::InsertSubtree { nodes, .. }) =
        import_html_snapshot().call(&args)
    else {
        panic!()
    };
    assert_eq!((nodes[0].base().x, nodes[0].base().y), (Some(0.), Some(3.)));
    assert_eq!(
        (nodes[1].base().x, nodes[1].base().y),
        (Some(24.), Some(17.))
    );
}
