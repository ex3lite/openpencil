//! Imported documents in managed mode must retain every authored node on edits.
use super::*;
use std::collections::BTreeMap;

fn own_node_properties(doc: &op_editor_core::PenDocument) -> BTreeMap<String, serde_json::Value> {
    let json = document_push::serialize_transport_document(doc).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut nodes = BTreeMap::new();
    let pages = value["pages"].as_array_mut().unwrap();
    let mut stack: Vec<serde_json::Value> = pages
        .iter_mut()
        .flat_map(|page| page["children"].as_array_mut().unwrap().drain(..))
        .collect();
    while let Some(mut node) = stack.pop() {
        if let Some(children) = node.as_object_mut().unwrap().remove("children") {
            stack.extend(children.as_array().unwrap().iter().cloned());
        }
        let id = node["id"].as_str().unwrap().to_string();
        assert!(nodes.insert(id, node).is_none(), "duplicate authored ID");
    }
    nodes
}

#[test]
#[ignore = "requires OPENBRUSH_LARGE_OP pointing to the complete UI Kit Italia fixture"]
fn managed_text_edits_preserve_every_full_kit_node_and_property() {
    let source = std::fs::read_to_string(std::env::var("OPENBRUSH_LARGE_OP").unwrap()).unwrap();
    let mut managed = fresh_state();
    managed.mode = ServeMode::Managed;
    managed.managed_token = Some("test-managed-token".into());
    managed
        .apply_document_push(
            &format!(r#"{{"document":{source},"preserveAuthoredGeometry":true}}"#),
            None,
        )
        .unwrap();
    let mut expected = own_node_properties(&managed.editor.doc);
    assert_eq!(expected.len(), 119191);
    assert_eq!(managed.editor.doc.pages.as_ref().unwrap().len(), 77);
    let state = Mutex::new(managed);
    for (page, id, text) in [
        (35, "fig_41699", "Button edit"),
        (76, "fig_119026", "Utilities edit"),
    ] {
        {
            let mut guard = state.lock().unwrap();
            guard.editor.ui.active_page_index = page;
        }
        let body = serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{
            "name":"update_node", "arguments":{"nodeId":id, "data":{"content":text}}
        }})
        .to_string();
        let request = format!("POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        let mut stream = mock_stream(&request);
        serve_one_in_mode(&mut stream, &state, &SseHub::default(), ServeMode::Managed).unwrap();
        let response = String::from_utf8(stream.output).unwrap();
        assert!(!response.contains(r#""isError":true"#), "{response}");
        expected.get_mut(id).unwrap()["content"] = text.into();
        let guard = state.lock().unwrap();
        let actual = own_node_properties(&guard.editor.doc);
        assert_eq!(
            actual.len(),
            expected.len(),
            "page {page}: narrow text edits must preserve every node"
        );
        assert_eq!(
            actual.keys().collect::<Vec<_>>(),
            expected.keys().collect::<Vec<_>>()
        );
        for (id, properties) in &expected {
            assert_eq!(
                actual.get(id),
                Some(properties),
                "page {page}: unrelated properties changed on {id}"
            );
        }
        assert!(guard.editor.editor_ui.preserve_authored_geometry);
        assert_eq!(guard.editor.doc.pages.as_ref().unwrap().len(), 77);
    }
}

#[test]
fn managed_text_edit_keeps_authored_mobile_chrome_and_local_mode_keeps_repairs() {
    for mode in [ServeMode::Managed, ServeMode::Local] {
        let mut state = fresh_state();
        state.mode = mode;
        state.apply_document_push(r##"{"document":{"version":"1.0","children":[],"pages":[{"id":"p","name":"Imported","children":[{"id":"screen","type":"frame","name":"Screen","width":375,"height":812,"children":[{"id":"authored-bar","type":"frame","name":"Status Bar","height":62,"children":[{"id":"authored-time","type":"text","content":"9:41"},{"id":"authored-icons","type":"text","content":"signal wifi battery"}]},{"id":"target","type":"text","content":"Before"}]}]}]},"preserveAuthoredGeometry":true}"##,None).unwrap();
        let mut expected = own_node_properties(&state.editor.doc);
        let state = Mutex::new(state);
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"update_node","arguments":{"nodeId":"target","data":{"content":"After"}}}}"#;
        let request = format!("POST /mcp HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len());
        let mut stream = mock_stream(&request);
        serve_one_in_mode(&mut stream, &state, &SseHub::default(), mode).unwrap();
        let state = state.lock().unwrap();
        let actual = own_node_properties(&state.editor.doc);
        assert_eq!(actual["target"]["content"], "After");
        assert!(state.editor.editor_ui.preserve_authored_geometry);
        if mode == ServeMode::Managed {
            expected.get_mut("target").unwrap()["content"] = "After".into();
            assert_eq!(
                actual, expected,
                "managed targeted edit must preserve authored chrome"
            );
        } else {
            assert!(
                !actual.contains_key("authored-bar"),
                "standalone mode retains its repair contract"
            );
            assert!(actual.contains_key("screen-status-bar"));
        }
    }
}
