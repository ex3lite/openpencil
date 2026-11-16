use super::*;

#[test]
fn unchanged_selection_is_republished_after_document_sync() {
    let state = op_editor_core::EditorState::starter();
    assert_eq!(
        selection_delivery_key(&state, 1),
        selection_delivery_key(&state, 1)
    );
    assert_ne!(
        selection_delivery_key(&state, 1),
        selection_delivery_key(&state, 2)
    );
}

fn minimal_document() -> op_editor_core::PenDocument {
    serde_json::from_str(r#"{"version":"1.0","children":[]}"#)
        .expect("minimal document should deserialize")
}

#[test]
fn capped_serializer_matches_regular_json_for_small_documents() {
    let doc = minimal_document();
    let mut expected = Vec::new();
    op_pen_loader::write_transport_document(&mut expected, &doc).unwrap();
    let expected = String::from_utf8(expected).unwrap();

    match serialize_sync_document(&doc) {
        SyncDocumentJson::Ready(actual) => assert_eq!(actual, expected),
        SyncDocumentJson::Oversize => panic!("small document should fit under the cap"),
        SyncDocumentJson::Failed => panic!("small document should serialize"),
    }
}

#[test]
fn capped_serializer_accepts_documents_above_old_two_mib_limit() {
    let mut doc = minimal_document();
    doc.name = Some("x".repeat(3 * 1024 * 1024));

    assert!(matches!(
        serialize_sync_document(&doc),
        SyncDocumentJson::Ready(_)
    ));
}

#[test]
fn capped_writer_accepts_exact_limit_then_rejects_another_byte() {
    let mut writer = CappedJsonWriter::new();
    writer.limit = 1024;
    let exact_limit = vec![b'x'; writer.limit];
    writer
        .write_all(&exact_limit)
        .expect("the exact cap should be accepted");
    assert_eq!(writer.bytes.len(), writer.limit);

    assert!(writer.write_all(b"x").is_err());
    assert!(writer.exceeded);
    assert_eq!(writer.bytes.len(), writer.limit);
}

#[test]
fn active_page_only_change_is_push_due_without_a_revision_change() {
    let mut sync = SyncController::new();
    let pair = (7, 11);
    sync.gate.note_synced(pair.0, pair.1);
    sync.client.mark_applied(3);
    sync.client.note_applied_snapshot_without_hash(0, true);

    assert!(!push_reasons(&sync, pair, 0, true).any());
    assert!(
        push_reasons(&sync, pair, 1, true).editor_meta,
        "page metadata must bypass the unchanged revision pair"
    );

    sync.gate.note_conflict(4);
    assert!(
        !push_reasons(&sync, pair, 1, true).any(),
        "metadata pushes must still honor the conflict latch"
    );
}

#[test]
#[ignore = "requires OPENBRUSH_LARGE_OP pointing to the complete UI Kit Italia fixture"]
fn complete_ui_kit_serializes_for_edit_sync_without_truncating_pages() {
    let path = std::env::var("OPENBRUSH_LARGE_OP").expect("fixture path");
    let source = std::fs::read_to_string(path).expect("read full kit");
    let started = std::time::Instant::now();
    let mut loaded = op_pen_loader::load_canonical(&source).expect("load full kit");
    assert_eq!(loaded.value.pages.as_ref().unwrap().len(), 77);
    let pages = loaded.value.pages.as_mut().unwrap();
    let last_page_name = pages[76].name.clone();
    pages[76].name = "Last page edit regression".into();
    let SyncDocumentJson::Ready(json) = serialize_sync_document(&loaded.value) else {
        panic!("the complete kit must remain synchronizable after an edit");
    };
    let response = format!(r#"{{"document":{json},"version":1}}"#);
    let pulled = WebSyncClient::new()
        .next_document_with_metadata(&response)
        .expect("web protocol must resolve the compact image table")
        .unwrap();
    assert_eq!(pulled.document.pages.as_ref().unwrap().len(), 77);
    let mut reopened = op_pen_loader::load_canonical(&json).expect("reopen edited full kit");
    let pages = reopened.value.pages.as_mut().unwrap();
    assert_eq!(pages.len(), 77);
    assert_eq!(pages[76].name, "Last page edit regression");
    pages[76].name = last_page_name;
    println!(
        "full_kit source_bytes={} sync_bytes={} pages={} elapsed_ms={}",
        source.len(),
        json.len(),
        pages.len(),
        started.elapsed().as_millis()
    );
}

#[test]
fn pending_page_survives_remote_update_and_reorder_without_losing_push() {
    let doc: op_editor_core::PenDocument = serde_json::from_value(serde_json::json!({
        "version":"1.0", "children":[], "pages":[
            {"id":"states","name":"States","children":[]},
            {"id":"references","name":"References","children":[]},
            {"id":"editor","name":"Editor","children":[]}
        ]
    }))
    .unwrap();
    let mut state = op_editor_core::EditorState::from_document(doc.clone());
    let mut client = WebSyncClient::new();
    assert_eq!(
        pending_local_page_id(&state, &client),
        None,
        "bootstrap follows daemon"
    );
    client.mark_applied(1);
    client.note_applied_editor_meta(0, false);
    assert_eq!(
        pending_local_page_id(&state, &client),
        None,
        "clean view follows daemon"
    );
    state.ui.active_page_index = 1;
    let pending = pending_local_page_id(&state, &client);
    assert_eq!(pending.as_deref(), Some("references"));
    assert_eq!(page_after_remote_apply(pending.as_deref(), &doc, 0), 1);
    let mut reordered = doc;
    reordered.pages.as_mut().unwrap().swap(1, 2);
    let local_index = page_after_remote_apply(pending.as_deref(), &reordered, 0);
    assert_eq!(local_index, 2);
    client.note_applied_editor_meta(0, false);
    assert!(
        client.editor_meta_needs_push(local_index, false),
        "local choice still needs publishing"
    );
    client.note_applied_editor_meta(local_index, false);
    assert!(!client.editor_meta_needs_push(local_index, false));
    reordered.pages.as_mut().unwrap().pop();
    assert_eq!(
        page_after_remote_apply(pending.as_deref(), &reordered, 0),
        0,
        "deleted local page follows remote"
    );
    assert_eq!(page_after_remote_apply(None, &reordered, 1), 1);
}
