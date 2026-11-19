use super::Html;
use jian_ops_schema::PenDocument;

fn render(source: &str) -> String {
    let doc: PenDocument = serde_json::from_str(source).unwrap();
    Html.generate_prepared(&doc)
}

fn element<'a>(html: &'a str, id: &str) -> &'a str {
    let (_, attrs) = html.split_once(&format!("data-op-id=\"{id}\"")).unwrap();
    attrs.split_once('>').unwrap().0
}

#[test]
fn row_and_column_share_remaining_space_and_keep_fixed_and_hugged_children() {
    let html = render(
        r#"{"version":"1","children":[{"id":"row","type":"frame","width":800,"height":50,"layout":"horizontal","padding":[4,12],"gap":6,"justifyContent":"space_between","alignItems":"center","children":[
        {"id":"fixed","type":"rectangle","x":0,"y":0,"width":72,"height":24},
        {"id":"col","type":"frame","width":"fill_container","height":"fill_container","layout":"vertical","padding":[1,2,3,4],"children":[{"id":"hug","type":"text","content":"Name","width":"fit_content","height":"fit_content"},{"id":"grow","type":"rectangle","width":"fill_container","height":"fill_container"}]},
        {"id":"overlay","type":"rectangle","role":"overlay","x":19,"y":2,"width":10,"height":10}
    ]}]}"#,
    );
    let row = element(&html, "row");
    for css in [
        "display:flex",
        "flex-direction:row",
        "padding:4px 12px",
        "gap:6px",
        "align-items:safe center",
        "justify-content:space-between",
        "width:800px",
        "max-width:100%",
    ] {
        assert!(row.contains(css), "{css}: {row}");
    }
    let fixed = element(&html, "fixed");
    assert!(fixed.contains("flex-shrink:0") && !fixed.contains("left:"));
    let col = element(&html, "col");
    for css in [
        "flex:1 1 0px",
        "align-self:stretch",
        "height:auto",
        "padding:1px 2px 3px 4px",
    ] {
        assert!(col.contains(css), "{css}: {col}");
    }
    let grow = element(&html, "grow");
    assert!(grow.contains("min-height:0") && grow.contains("width:auto"));
    assert!(element(&html, "hug").contains("width:fit-content;height:fit-content"));
    assert!(element(&html, "overlay").contains("position:absolute;left:19px;top:2px"));
}

#[test]
fn text_keeps_its_ink_metrics_wrapping_and_styled_runs() {
    let html = render(
        r##"{"version":"1","children":[{"id":"label","type":"text","width":160,"height":40,"content":[{"text":"A < B","fontWeight":700},{"text":"  Next","fill":"#f00"}],"textGrowth":"fixed-width","fontFamily":"Some Font","fontSize":15,"fontWeight":"medium","letterSpacing":0.5,"lineHeight":1.4,"textAlign":"right","textAlignVertical":"middle","fill":[{"type":"solid","color":"#102030","opacity":0.5}]}]}"##,
    );
    let label = element(&html, "label");
    for css in [
        "color:rgba(16,32,48,0.500)",
        "font-family:'Some Font'",
        "font-size:15px",
        "font-weight:500",
        "letter-spacing:0.5px",
        "line-height:1.4",
        "text-align:right",
        "align-content:center",
        "white-space:pre-wrap",
    ] {
        assert!(label.contains(css), "{css}: {label}");
    }
    assert!(!label.contains("background:"));
    assert!(html.contains("<span style=\"font-weight:700\">A &lt; B</span>"));
    assert!(html.contains("<span style=\"color:rgb(255,0,0)\">  Next</span>"));
}

#[test]
fn svg_scales_source_bounds_without_scaling_the_stroke() {
    let html = render(
        r##"{"version":"1","children":[{"id":"glyph","type":"path","x":3,"y":2,"width":12,"height":9,"d":"M 10 20 L 30 35","stroke":{"cap":"round","join":"bevel","thickness":1.5,"fill":[{"type":"solid","color":"#234"}]}},{"id":"drawn","type":"path","anchors":[{"x":2,"y":3},{"x":12,"y":10}],"closed":true,"fillRule":"evenodd","fill":[{"type":"solid","color":"#123"}]}]}"##,
    );
    assert!(element(&html, "glyph").contains("viewBox=\"10 20 20 15\""));
    assert!(html.contains("d=\"M 10 20 L 30 35\" fill=\"none\" stroke=\"rgb(34,51,68)\" stroke-width=\"1.5\" vector-effect=\"non-scaling-stroke\" stroke-linecap=\"round\" stroke-linejoin=\"bevel\""));
    assert!(
        html.contains("d=\"M 2 3 L 12 10 L 2 3 Z\" fill=\"rgb(17,34,51)\" fill-rule=\"evenodd\"")
    );
}

#[test]
fn css_tokens_stay_editable_for_paints_and_numeric_dimensions() {
    let html = render(
        r##"{"version":"1","variables":{"--ink":{"type":"color","value":"#abcdef"},"ink/sub":{"type":"color","value":"#f4f5f6"},"ink-sub":{"type":"color","value":"#ffffff"},"accent":{"type":"color","value":"$--ink"},"space":{"type":"number","value":8},"unsafe":{"type":"color","value":"</style><script>alert(1)</script>"}},"children":[{"type":"frame","id":"card","width":"$space","height":28,"layout":"horizontal","padding":"$space","gap":"$space","opacity":"$space","fill":[{"type":"solid","color":"$accent","opacity":0.65}]}]}"##,
    );
    for s in [
        "--ink: #abcdef",
        r"--ink\2f sub: #f4f5f6",
        "--ink-sub: #ffffff",
        "--accent: var(--ink)",
        "--space: 8",
        "width:calc(var(--space) * 1px)",
        "gap:calc(var(--space) * 1px)",
        "padding:calc(var(--space) * 1px)",
        "background:color-mix(in srgb,var(--accent) 65%,transparent)",
        "opacity:var(--space)",
    ] {
        assert!(html.contains(s), "{s} in {html}");
    }
    assert!(!html.contains("<script>"));
}

#[test]
fn standalone_frame_subtracts_its_inset_from_the_available_width() {
    let html = render(
        r#"{"version":"1","children":[{"id":"panel","type":"frame","layout":"horizontal","x":16,"width":1408,"height":56,"maxWidth":1600,"children":[{"id":"child","type":"frame","width":180,"layout":"horizontal","maxWidth":200}]}]}"#,
    );
    let root = element(&html, "panel");
    assert!(root.contains("left:16px"));
    assert!(root.contains("width:1408px"));
    assert!(root.contains("max-width:min(1600px,calc(100% - 16px))"));
    let child = element(&html, "child");
    assert!(child.contains("max-width:200px"));
    assert!(!child.contains("100%"));
}

#[test]
fn selected_frames_use_local_coordinates_and_docked_panels_keep_their_gutter() {
    use op_editor_core::PenNodeExt;
    let doc: PenDocument = serde_json::from_str(r#"{"version":"1","children":[{"id":"board","type":"frame","x":1880,"y":2160,"width":1440,"height":960,"layout":"none","children":[
        {"id":"inspector","type":"frame","x":1152,"y":88,"width":272,"height":856,"layout":"vertical"},
        {"id":"bar","type":"frame","x":16,"y":16,"width":1408,"height":56,"layout":"horizontal"}
    ]}]}"#).unwrap();
    let root = &doc.children[0];
    let kids = root.children().unwrap();
    let export = |node| Html.generate_selection(&doc, [node], &Default::default());
    let board = export(root);
    assert!(element(&board, "board").contains("left:0px;top:0px"));
    assert!(element(&board, "inspector").contains("left:1152px;top:88px"));
    let inspector = export(&kids[0]);
    assert!(element(&inspector, "inspector").contains("left:0px;top:0px;width:272px"));
    assert!(element(&inspector, "inspector").contains("max-width:100%"));
    let bar = export(&kids[1]);
    assert!(element(&bar, "bar").contains("left:16px;top:16px"));
    assert!(element(&bar, "bar").contains("max-width:calc(100% - 16px)"));
    let saved = op_util::editable_html::unchanged_document_source(&board).unwrap();
    let source: PenDocument = serde_json::from_str(saved).unwrap();
    assert_eq!(source.children[0].base().x, Some(1880.0));
}

#[test]
fn sibling_layers_paint_topmost_first_without_reversing_flex_or_focus_order() {
    let html = render(
        r#"{"version":"1","children":[{"id":"row","type":"frame","layout":"horizontal","children":[
        {"id":"label","type":"text","content":"Caption"},{"id":"surface","type":"frame","children":[{"id":"nested","type":"rectangle"}]}
    ]},{"id":"back","type":"rectangle"}]}"#,
    );
    for (id, index) in [
        ("row", 1),
        ("label", 1),
        ("surface", 0),
        ("nested", 0),
        ("back", 0),
    ] {
        assert!(element(&html, id).contains(&format!("z-index:{index}")));
    }
    assert!(html.find("data-op-id=\"label\"") < html.find("data-op-id=\"surface\""));
}

#[test]
fn open_row_grows_below_the_original_alignment_box_for_taller_flow_children() {
    let html = render(
        r#"{"version":"1","children":[{"type":"frame","id":"row","height":32,"layout":"horizontal","alignItems":"center","children":[
        {"type":"rectangle","id":"small","height":32},{"type":"rectangle","id":"large","height":36},
        {"type":"rectangle","id":"overlay","role":"overlay","height":120}
    ]}]}"#,
    );
    let row = element(&html, "row");
    assert!(row.contains("height:36px;padding-bottom:4px"));
    assert!(row.contains("align-items:safe center"));
}

#[test]
fn positioned_vector_snaps_its_two_edges_independently() {
    let html = render(
        r#"{"version":"1","children":[{"type":"path","id":"dots","x":11.3,"y":11.3,"width":1417.4,"height":937.4,"d":"M0 0 L1417.4 937.4"}]}"#,
    );
    assert!(element(&html, "dots").contains("left:11px;top:11px;width:1418px;height:938px"));
}
