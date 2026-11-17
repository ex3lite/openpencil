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

#[test]
#[ignore = "requires OPENBRUSH_PROTOTYPE with the authored project/chat flows"]
fn authored_project_chat_flows_are_clickable_and_respect_reduced_motion() {
    let source = std::fs::read_to_string(std::env::var("OPENBRUSH_PROTOTYPE").unwrap()).unwrap();
    let document = jian_ops_schema::load_str(&source).unwrap().value;
    let mut session = PreviewSession::enter(
        &document,
        (1800.0, 992.0),
        &Default::default(),
        0,
        false,
        false,
        jian_core::layout::measure::default_backend(),
        0,
    )
    .unwrap();
    for (i, route) in [
        "/fullscreen",
        "/documents",
        "/documents-empty",
        "/sent",
        "/run",
    ]
    .iter()
    .enumerate()
    {
        let now = 1000 + i as u64 * 1000;
        session.set_now_ms(now);
        session.pump(now);
        session.navigate_to_screen(route);
        assert!(session.reconcile(now).switched, "{route}");
        assert!(session.current_screen_scene_rect().is_some(), "{route}");
        session.begin_lifecycle(now);
        if *route == "/documents" {
            assert!(session.active_animation_track_count() > 0);
        }
        session.pump(now + 500);
        assert_eq!(session.active_animation_track_count(), 0, "{route}");
    }
    session.set_motion_preference(jian_ops_schema::MotionPreference::Reduced);
    session.navigate_to_screen("/documents");
    session.reconcile(7000);
    session.begin_lifecycle(7000);
    assert_eq!(session.active_animation_track_count(), 0);
    // Click the visible close button, rather than merely assigning a route.
    let close_id = std::env::var("OPENBRUSH_CLOSE_ID").unwrap();
    let (x, y, w, h) = session
        .node_rect(&close_id)
        .expect("project picker close button");
    let origin = session.current_screen_scene_rect().unwrap().origin;
    session.set_now_ms(7500);
    session.pump(7500);
    assert!(
        session.dispatch_tap(origin.x + x + w / 2.0, origin.y + y + h / 2.0),
        "rect {x} {y} {w} {h}; origin {origin:?}"
    );
    session.set_now_ms(8000);
    session.pump(8000);
    assert!(session.reconcile(8000).switched);
    assert!(
        session.node_rect("ob-flow-composer").is_some(),
        "returns to full screen with chat"
    );
    assert!(session.warnings().is_empty(), "{:?}", session.warnings());
}

#[test]
fn front_modal_wins_over_deeper_background_in_preview() {
    let document = jian_ops_schema::load_str(r#"{"version":"0.8.5","pages":[{"id":"p","name":"P","children":[
      {"id":"home","type":"frame","screen":"/","width":300,"height":300,"children":[
        {"id":"modal-button","type":"frame","x":20,"y":20,"width":100,"height":40,"events":{"onTap":[{"push":"\"/done\""}]},"children":[]},
        {"id":"background","type":"frame","width":300,"height":300,"children":[{"id":"deep","type":"frame","width":300,"height":300,"children":[{"id":"deepest","type":"rectangle","width":300,"height":300}]}]}
      ]},
      {"id":"done","type":"frame","screen":"/done","x":400,"width":300,"height":300,"children":[]}
    ]}]}"#).unwrap().value;
    let mut session = PreviewSession::enter(
        &document,
        (300.0, 300.0),
        &Default::default(),
        0,
        false,
        false,
        jian_core::layout::measure::default_backend(),
        0,
    )
    .unwrap();
    session.set_now_ms(1000);
    session.pump(1000);
    assert!(session.dispatch_tap(70.0, 40.0));
    assert!(
        session.reconcile(1000).switched,
        "visible modal must receive the tap"
    );
}
