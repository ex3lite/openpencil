use super::PreviewSession;

#[test]
#[ignore = "requires OPENBRUSH_PROTOTYPE pointing to an authored .op fixture"]
fn authored_prototype_routes_and_mount_motion() {
    let source = std::fs::read_to_string(std::env::var("OPENBRUSH_PROTOTYPE").unwrap()).unwrap();
    let document = jian_ops_schema::load_str(&source).unwrap().value;
    let mut session = PreviewSession::enter(
        &document,
        (1440.0, 960.0),
        &Default::default(),
        0,
        false,
        false,
        jian_core::layout::measure::default_backend(),
        0,
    )
    .unwrap();
    assert!(session.is_app_mode());
    session.begin_lifecycle(0);
    session.set_now_ms(1000);
    session.pump(1000);
    let (x, y, w, h) = session
        .node_rect("ob-instance-133")
        .expect("library action");
    assert!(session.dispatch_tap(x + w / 2.0, y + h / 2.0));
    assert!(session.reconcile(1000).switched, "a real tap must navigate");
    assert!(session.node_rect("ob-f-557").is_some(), "library opens");
    session.begin_lifecycle(1000);
    assert!(
        session.active_animation_track_count() > 0,
        "library mount motion starts"
    );
    session.pump(1200);
    assert_eq!(session.active_animation_track_count(), 0);
    session.set_motion_preference(jian_ops_schema::MotionPreference::Reduced);
    for (index, route) in ["/context", "/compact", "/preview", "/"].iter().enumerate() {
        let now = 2000 + index as u64 * 1000;
        session.set_now_ms(now);
        session.pump(now);
        session.navigate_to_screen(route);
        assert!(session.reconcile(now).switched, "route {route}");
        assert!(session.current_screen_scene_rect().is_some());
        session.begin_lifecycle(now);
        assert_eq!(
            session.active_animation_track_count(),
            0,
            "reduced motion is instant"
        );
    }
    assert!(session.warnings().is_empty(), "{:?}", session.warnings());
}
