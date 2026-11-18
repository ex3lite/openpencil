use super::WidgetHost;
use op_editor_core::PenNodeExt;
use op_editor_ui::widgets::LayerPanelHit;
use op_editor_ui::Point2D;

const VIEWPORT_W: f32 = 1200.0;
const VIEWPORT_H: f32 = 800.0;

fn page_switch_host() -> WidgetHost {
    let mut host = WidgetHost::new();
    let doc = jian_ops_schema::load_str(
        r#"{"version":"1.0.0","children":[],"pages":[
            {"id":"p1","name":"Near","children":[
                {"type":"rectangle","id":"near","name":"Near","x":0,"y":0,
                 "width":100,"height":100}]},
            {"id":"p2","name":"Far","children":[
                {"type":"frame","id":"far","name":"Far","x":2000,"y":1500,
                 "width":400,"height":200,"clipContent":false,"children":[
                    {"type":"rectangle","id":"spill","x":1200,"y":20,
                     "width":100,"height":100}]}]}
        ]}"#,
    )
    .expect("page fixture parses")
    .value;
    host.editor_state = op_editor_core::EditorState::from_document(doc);
    host.editor_state.editor_ui.preserve_authored_geometry = true;
    host.editor_state_dirty = true;
    host
}

fn point_for_page_row(host: &WidgetHost, page_index: usize) -> Point2D {
    let panel = host.layer_panel();
    let rect = host.layer_panel_rect(VIEWPORT_H);
    let regions = panel.regions(rect);
    let mut y = regions.pages_rows_top + 2.0;
    while y < regions.pages_rows_top + regions.pages_view_h {
        let point = Point2D::new(rect.origin.x + 48.0, y);
        if matches!(
            panel.hit_test(rect, point),
            Some(LayerPanelHit::Page(index)) if index == page_index
        ) {
            return point;
        }
        y += 2.0;
    }
    panic!("no page row point found for index {page_index}");
}

#[test]
fn page_row_switch_fits_new_page_after_cached_scene() {
    let mut host = page_switch_host();

    // Match a real post-paint host: page 0 owns the derived scene and the host
    // dirty bit has been consumed before the user switches pages.
    host.refresh_layout_scene();
    assert_eq!(host.layout_scene.active_page_index, 0);
    assert!(!host.editor_state_dirty);

    let near_rows = host.layer_panel().items;
    let near_rows_again = host.layer_panel().items;
    assert!(
        std::rc::Rc::ptr_eq(&near_rows, &near_rows_again),
        "event-time panel resolves must reuse this host's owned row cache"
    );

    let page_row = point_for_page_row(&host, 1);
    assert!(host.apply_press(page_row.x, page_row.y, VIEWPORT_W, VIEWPORT_H,));
    assert_eq!(host.editor_state.ui.active_page_index, 1);

    let far_rows = host.layer_panel().items;
    assert!(
        !std::rc::Rc::ptr_eq(&near_rows, &far_rows),
        "the active-page cache key must rebuild rows after a switch"
    );
    assert!(far_rows.iter().any(|row| row.node_id.as_str() == "far"));
    assert!(std::rc::Rc::ptr_eq(&far_rows, &host.layer_panel().items));

    // Resolve the active scene so the assertion derives its expected centre
    // from the same layout path used by paint, not from authored fixture math.
    host.refresh_layout_scene();
    let bounds = host
        .layout_scene
        .content_bounds()
        .expect("the switched-to page has content");
    let (_, _, canvas_w, canvas_h) = host.canvas_region(VIEWPORT_W, VIEWPORT_H);
    let viewport = host.editor_state.viewport;
    let content_center_x = bounds.origin.x + bounds.size.x / 2.0;
    let content_center_y = bounds.origin.y + bounds.size.y / 2.0;
    let expected_zoom = ((canvas_w - 128.0).max(1.0) / bounds.size.x)
        .min((canvas_h - 128.0).max(1.0) / bounds.size.y)
        .clamp(0.1, 1.0);

    assert!(
        bounds.size.x > 1200.0,
        "explicitly open frame must include descendant overflow: {bounds:?}"
    );
    assert!(
        (viewport.zoom - expected_zoom).abs() < 0.0001,
        "fit scale must include open-frame overflow: viewport={viewport:?}, bounds={bounds:?}"
    );

    assert!(
        (viewport.pan_x + content_center_x * viewport.zoom - canvas_w / 2.0).abs() < 0.01,
        "new page must be horizontally centered: viewport={viewport:?}, bounds={bounds:?}"
    );
    assert!(
        (viewport.pan_y + content_center_y * viewport.zoom - canvas_h / 2.0).abs() < 0.01,
        "new page must be vertically centered: viewport={viewport:?}, bounds={bounds:?}"
    );
}

#[test]
fn clicking_active_page_row_preserves_viewport() {
    let mut host = page_switch_host();
    host.editor_state.viewport = op_editor_core::Viewport {
        pan_x: -321.0,
        pan_y: 147.0,
        zoom: 0.42,
    };
    host.refresh_layout_scene();
    let viewport_before = host.editor_state.viewport;

    let active_page_row = point_for_page_row(&host, 0);
    assert!(host.apply_press(active_page_row.x, active_page_row.y, VIEWPORT_W, VIEWPORT_H,));

    assert_eq!(host.editor_state.ui.active_page_index, 0);
    assert_eq!(
        host.editor_state.viewport, viewport_before,
        "reselecting the active page must not zoom-to-fit"
    );
}

#[cfg(feature = "canvaskit")]
fn start_page_preview(host: &mut WidgetHost) {
    host.preview = Some(
        op_preview_core::PreviewSession::enter(
            &host.editor_state.doc,
            (VIEWPORT_W, VIEWPORT_H),
            &std::collections::BTreeMap::new(),
            host.editor_state.ui.active_page_index,
            false,
            false,
            jian_core::layout::measure::default_backend(),
            0,
        )
        .expect("preview"),
    );
    host.editor_state.editor_ui.preview.mode = true;
}

#[cfg(feature = "canvaskit")]
#[test]
fn switching_page_drops_the_previous_pages_preview_scene() {
    let mut host = page_switch_host();
    host.refresh_layout_scene();
    start_page_preview(&mut host);
    let point = point_for_page_row(&host, 1);
    assert!(host.apply_press(point.x, point.y, VIEWPORT_W, VIEWPORT_H));
    assert_eq!(host.editor_state.ui.active_page_index, 1);
    assert!(
        host.preview.is_none(),
        "paint must not keep the previous page's preview scene"
    );
    assert!(!host.editor_state.editor_ui.preview.mode);
    host.refresh_layout_scene();
    assert!(host
        .layout_scene
        .active_page()
        .unwrap()
        .find("far")
        .is_some());
}

#[cfg(feature = "canvaskit")]
#[test]
fn remote_document_replacement_drops_stale_preview() {
    let mut host = page_switch_host();
    host.refresh_layout_scene();
    start_page_preview(&mut host);
    let replacement = host.editor_state.doc.clone();
    host.replace_document_from_sync(replacement, true);
    assert!(
        host.preview.is_none(),
        "remote content cannot leave an old runtime painting"
    );
    assert!(!host.editor_state.editor_ui.preview.mode);
}

#[cfg(feature = "canvaskit")]
#[test]
#[ignore = "requires OPENBRUSH_PROTOTYPE pointing to an authored .op fixture"]
fn authored_document_all_pages_have_matching_scene_and_layers() {
    let source = std::fs::read_to_string(std::env::var("OPENBRUSH_PROTOTYPE").unwrap()).unwrap();
    let mut host = WidgetHost::new();
    host.editor_state = op_editor_core::EditorState::from_document(
        jian_ops_schema::load_str(&source).unwrap().value,
    );
    let pages = host.editor_state.doc.pages.as_ref().unwrap().clone();
    for index in [4, 3, 5, 0, 2, 1, 4, 3, 0] {
        host.editor_state.editor_ui.layer_pages_scroll.offset = (index as f32 * 32.0).max(0.0);
        host.mark_editor_state_dirty();
        host.refresh_layout_scene();
        let point = point_for_page_row(&host, index);
        assert!(host.apply_press(point.x, point.y, VIEWPORT_W, VIEWPORT_H));
        host.refresh_layout_scene();
        assert_eq!(host.editor_state.ui.active_page_index, index);
        let scene = host.layout_scene.active_page().unwrap();
        assert_eq!(scene.id, pages[index].id);
        for root in &pages[index].children {
            assert!(
                scene.find(root.id_str()).is_some(),
                "missing root on page {index}"
            );
        }
        assert!(host.preview.is_none());
    }
}

#[test]
fn codex_comment_reveal_selects_and_centers_nested_node_atomically() {
    let mut host = page_switch_host();
    let doc_revision = host.editor_state.revision;
    assert!(host.reveal_codex_layers("p2", &["spill".into()], VIEWPORT_W, VIEWPORT_H));
    assert_eq!(host.editor_state.ui.active_page_index, 1);
    assert_eq!(host.editor_state.selection.anchor.as_str(), "spill");
    let scene = host.layout_scene.active_page().unwrap();
    let bounds = scene.find("spill").unwrap().aggregate_bounds();
    let view = host.editor_state.viewport;
    let (_, _, width, height) = host.canvas_region(VIEWPORT_W, VIEWPORT_H);
    assert!(
        (view.pan_x + (bounds.origin.x + bounds.size.x * 0.5) * view.zoom - width * 0.5).abs()
            < 0.01
    );
    assert!(
        (view.pan_y + (bounds.origin.y + bounds.size.y * 0.5) * view.zoom - height * 0.5).abs()
            < 0.01
    );
    assert!(!host.reveal_codex_layers(
        "p1",
        &["near".into(), "missing".into()],
        VIEWPORT_W,
        VIEWPORT_H
    ));
    assert_eq!(host.editor_state.ui.active_page_index, 1);
    assert_eq!(host.editor_state.selection.anchor.as_str(), "spill");
    assert_eq!(host.editor_state.viewport, view);
    assert_eq!(host.editor_state.revision, doc_revision);
}

#[test]
fn codex_scrollbar_matches_panel_examples_and_consumes_wheel() {
    let rows: Vec<_> = (0..24)
        .map(|id| {
            serde_json::json!({
                "type":"rectangle", "id":format!("n{id}"), "name":"A long name ".repeat(40),
                "x":id * 100,"y":0, "width":90, "height":80
            })
        })
        .collect();
    let doc = jian_ops_schema::load_str(
        &serde_json::json!({
            "version":"1.0.0","children":[],"pages":[
                {"id":"p0","name":"A","children":rows},
                {"id":"p1","name":"B","children":[]},
                {"id":"p2","name":"C","children":[]}]
        })
        .to_string(),
    )
    .unwrap()
    .value;
    let mut host = WidgetHost::new();
    host.editor_state = op_editor_core::EditorState::from_document(doc);
    host.editor_state.editor_ui.embed = op_editor_core::EmbedHost::Codex;
    host.editor_state.editor_ui.sidebar_open = true;
    host.last_viewport_w = 1440.0;
    host.last_viewport_h = 704.0;
    let panel = host.layer_panel();
    let rect = host.layer_panel_rect(704.0);
    let region = panel.regions(rect);
    assert_eq!(region.layers_rows_top, 336.0);
    assert_eq!(region.layers_view_h, 344.0);
    let thumb = region.layer_thumb_rect(rect).unwrap();
    assert_eq!(thumb, op_editor_ui::Rect::xywh(242.0, 336.0, 6.0, 154.0));
    assert!(panel.hit_test(rect, Point2D::new(231.0, 345.0)).is_some());
    assert!(panel.hit_test(rect, Point2D::new(232.0, 345.0)).is_none());
    let camera = host.editor_state.viewport;
    assert!(host.try_scroll_layer_panel(150.0, 345.0, -120.0, 0.0, 704.0));
    assert_eq!(
        host.editor_state.editor_ui.layer_layers_h_scroll.offset,
        0.0
    );
    assert_eq!(host.editor_state.viewport, camera);
    assert!(host.apply_press(244.0, 348.0, 1440.0, 704.0));
    assert!(host.layer_thumb_grab.is_some());
    assert!(host.apply_cursor_move(244.0, 434.0));
    let scrolled = host.layer_panel().regions(rect);
    assert!((scrolled.layers.offset - 192.0).abs() < 1.0);
    assert_eq!(scrolled.layer_thumb_rect(rect).unwrap().origin.y, 422.0);
    assert!(host.apply_release_with_viewport(1440.0, 704.0));
    assert!(host.layer_thumb_grab.is_none());
    assert!(host.editor_state.selection.is_empty());
}
