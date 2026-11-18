//! The exact native control preset shipped by the Codex plugin.
use super::{test_measure, PreviewSession};
use jian_core::gesture::pointer::{PointerKind, PointerPhase};
fn session() -> PreviewSession {
    let doc = jian_ops_schema::load_str(include_str!("fixtures/openbrush-ui-kit.op"))
        .expect("native kit parses")
        .value;
    PreviewSession::enter(
        &doc,
        (400.0, 300.0),
        &Default::default(),
        0,
        false,
        false,
        test_measure(),
        0,
    )
    .expect("native kit previews")
}
fn opacity(session: &PreviewSession, id: &str) -> f32 {
    session
        .preview_scene_for_test()
        .active_page()
        .unwrap()
        .find(id)
        .unwrap()
        .opacity
}
#[test]
fn linked_button_hover_press_and_cancel_animate_without_layout_changes() {
    let mut s = session();
    assert!((opacity(&s, "button") - 1.0).abs() < 0.001);
    s.dispatch_pointer_for_id_at(1, PointerKind::Mouse, 100.0, 45.0, PointerPhase::Hover, 0);
    let _ = opacity(&s, "button");
    s.pump(150);
    assert!((opacity(&s, "independent") - 1.0).abs() < 0.001);
    assert!(
        (opacity(&s, "button") - 0.94).abs() < 0.01,
        "hover {}",
        opacity(&s, "button")
    );
    s.dispatch_pointer_for_id_at(1, PointerKind::Mouse, 100.0, 45.0, PointerPhase::Down, 151);
    let _ = opacity(&s, "button");
    s.pump(301);
    assert!(
        (opacity(&s, "button") - 0.78).abs() < 0.01,
        "press {}",
        opacity(&s, "button")
    );
    s.dispatch_pointer_for_id_at(
        1,
        PointerKind::Mouse,
        100.0,
        45.0,
        PointerPhase::Cancel,
        302,
    );
    let _ = opacity(&s, "button");
    s.pump(452);
    assert!(opacity(&s, "button") > 0.8);
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
}
#[test]
fn linked_button_focus_and_disabled_are_real_runtime_states() {
    let mut s = session();
    s.focus_next();
    let _ = opacity(&s, "button");
    s.pump(150);
    assert!(
        (opacity(&s, "button") - 0.86).abs() < 0.01,
        "focus {}",
        opacity(&s, "button")
    );
    assert!((opacity(&s, "disabled") - 0.35).abs() < 0.01);
    s.dispatch_pointer_for_id_at(1, PointerKind::Mouse, 100.0, 125.0, PointerPhase::Down, 151);
    s.pump(301);
    assert!((opacity(&s, "disabled") - 0.35).abs() < 0.01);
}
#[test]
fn reduced_motion_reaches_hover_state_without_transition_tracks() {
    let mut s = session();
    s.set_motion_preference(jian_ops_schema::MotionPreference::Reduced);
    s.dispatch_pointer_for_id_at(1, PointerKind::Mouse, 100.0, 45.0, PointerPhase::Hover, 0);
    assert!((opacity(&s, "button") - 0.94).abs() < 0.01);
    assert_eq!(s.active_animation_track_count(), 0);
}

#[test]
fn button_label_is_centered_by_layout_without_authored_offsets() {
    let s = session();
    let (x, y, w, h) = s.node_rect("button").unwrap();
    let (tx, ty, tw, th) = s.node_rect("button__brush-button-primary-label").unwrap();
    assert!(
        (tx + tw / 2.0 - x - w / 2.0).abs() < 0.5,
        "horizontal centering"
    );
    assert!(
        (ty + th / 2.0 - y - h / 2.0).abs() < 0.5,
        "vertical centering"
    );
}

#[test]
fn changing_a_shared_token_and_master_updates_cross_page_instances() {
    let mut source: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/openbrush-ui-kit.op")).unwrap();
    source["variables"]["brush-accent"]["value"] = serde_json::json!("#FF9900");
    source["pages"][1]["children"][0]["children"][0]["content"] = serde_json::json!("Новая кнопка");
    let doc = jian_ops_schema::load_str(&source.to_string())
        .unwrap()
        .value;
    let s = PreviewSession::enter(
        &doc,
        (400.0, 300.0),
        &Default::default(),
        0,
        false,
        false,
        test_measure(),
        0,
    )
    .unwrap();
    let scene = s.preview_scene_for_test();
    let page = scene.active_page().unwrap();
    for id in ["button", "disabled"] {
        let node = page.find(id).unwrap();
        let fill = node.fill.unwrap();
        assert!(fill.r > 0.99 && fill.g > 0.59 && fill.g < 0.61 && fill.b < 0.01);
        assert_eq!(node.children[0].text.as_deref(), Some("Новая кнопка"));
    }
}

#[test]
fn leaving_the_host_surface_runs_hover_leave_and_can_enter_again() {
    let mut s = session();
    let hover = |s: &mut PreviewSession, time| {
        s.dispatch_pointer_for_id_at(
            1,
            PointerKind::Mouse,
            100.0,
            45.0,
            PointerPhase::Hover,
            time,
        );
        let _ = opacity(s, "button");
        s.pump(time + 150);
        assert!((opacity(s, "button") - 0.94).abs() < 0.01);
    };
    hover(&mut s, 0);
    assert!(s.clear_hover());
    assert!(!s.clear_hover());
    let _ = opacity(&s, "button");
    s.pump(320);
    assert!((opacity(&s, "button") - 1.0).abs() < 0.01);
    hover(&mut s, 400);
}
