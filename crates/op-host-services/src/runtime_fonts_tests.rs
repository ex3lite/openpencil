use super::*;

const REGULAR: &[u8] = include_bytes!("../../op-host-desktop/assets/fonts/DMMono-Regular.ttf");
const MEDIUM: &[u8] = include_bytes!("../../op-host-desktop/assets/fonts/DMMono-Medium.ttf");
const INTER: &[u8] = include_bytes!("../../op-host-desktop/assets/fonts/Inter-VF.ttf");

fn request(fonts: &[&[u8]]) -> String {
    let fonts: Vec<_> = fonts
        .iter()
        .map(|bytes| {
            serde_json::json!({
                "dataBase64":base64::engine::general_purpose::STANDARD.encode(bytes)
            })
        })
        .collect();
    serde_json::json!({"fonts":fonts}).to_string()
}

#[test]
fn exact_bytes_weights_glyph_coverage_and_idempotency_reach_renderer() {
    let mut fonts = RuntimeFonts::default();
    fonts.register(&request(&[REGULAR, MEDIUM, INTER])).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&fonts.manifest()).unwrap();
    assert_eq!(manifest["faces"].as_array().unwrap().len(), 3);
    for face in manifest["faces"].as_array().unwrap() {
        let hash = face["sha256"].as_str().unwrap();
        let bytes = fonts.bytes(&format!("/api/mcp/fonts/{hash}")).unwrap();
        assert!([REGULAR, MEDIUM, INTER].contains(&bytes));
        assert!(face["nativeRegistered"].as_bool().unwrap());
        let skia = skia_safe::FontMgr::new()
            .new_from_data(bytes, None)
            .unwrap();
        let missing: String = op_editor_core::runtime_font::RUSSIAN_ALPHABET
            .chars()
            .filter(|c| skia.unichar_to_glyph(*c as i32) == 0)
            .collect();
        assert_eq!(face["cyrillic"], missing.is_empty());
        assert_eq!(face["missingCyrillic"], missing);
    }
    let weights: Vec<_> = manifest["faces"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|face| face["family"] == "DM Mono")
        .map(|face| face["weight"].as_u64().unwrap())
        .collect();
    assert!(weights.contains(&400) && weights.contains(&500));
    fonts.register(&request(&[REGULAR, MEDIUM, INTER])).unwrap();
    assert_eq!(fonts.manifest(), serde_json::to_string(&manifest).unwrap());
    let provider: skia_safe::FontMgr = jian_skia::bundled_fonts::imported_provider()
        .unwrap()
        .into();
    let bold = provider
        .match_family_style("Inter", skia_safe::FontStyle::bold())
        .unwrap();
    assert_eq!(*bold.font_style().weight(), 700);
}

#[test]
fn untrusted_family_path_url_and_invalid_batch_cannot_mutate_font_state() {
    let mut fonts = RuntimeFonts::default();
    for request in [
        r#"{"fonts":[{"url":"https://example.invalid/font.ttf"}]}"#,
        r#"{"fonts":[{"path":"/tmp/font.ttf"}]}"#,
        r#"{"fonts":[{"family":"Aliased","dataBase64":"AA=="}]}"#,
    ] {
        assert!(fonts.register(request).is_err());
    }
    let valid = base64::engine::general_purpose::STANDARD.encode(REGULAR);
    let invalid_batch =
        serde_json::json!({"fonts":[{"dataBase64":valid},{"dataBase64":"AA=="}]}).to_string();
    assert!(fonts.register(&invalid_batch).is_err());
    assert_eq!(fonts.generation, 0);
    assert!(fonts.bytes("/api/mcp/fonts/../../etc/passwd").is_none());
}

#[test]
fn browser_readiness_is_separate_from_native_and_requires_all_current_hashes() {
    let mut fonts = RuntimeFonts::default();
    fonts.register(&request(&[REGULAR])).unwrap();
    let hash = fonts.faces.keys().next().unwrap().clone();
    assert!(
        !serde_json::from_str::<serde_json::Value>(&fonts.manifest()).unwrap()["browser"]["ready"]
            .as_bool()
            .unwrap()
    );
    fonts
        .note_browser_readiness(
            &serde_json::json!({"generation":fonts.generation,"loadedSha256":[hash]}).to_string(),
        )
        .unwrap();
    assert!(
        serde_json::from_str::<serde_json::Value>(&fonts.manifest()).unwrap()["browser"]["ready"]
            .as_bool()
            .unwrap()
    );
    fonts.register(&request(&[MEDIUM])).unwrap();
    assert!(
        !serde_json::from_str::<serde_json::Value>(&fonts.manifest()).unwrap()["browser"]["ready"]
            .as_bool()
            .unwrap()
    );
    assert!(fonts
        .note_browser_readiness(r#"{"generation":0,"loadedSha256":[]}"#)
        .is_err());
}

#[test]
#[ignore = "requires OPENBRUSH_FONT_NORMAL and OPENBRUSH_FONT_ITALIC real acquisition fixtures"]
fn acquired_native_fonts_cover_all_russian_glyphs_and_preserve_real_styles() {
    let normal = std::fs::read(std::env::var("OPENBRUSH_FONT_NORMAL").unwrap()).unwrap();
    let italic = std::fs::read(std::env::var("OPENBRUSH_FONT_ITALIC").unwrap()).unwrap();
    let normal_meta = op_editor_core::runtime_font::parse(&normal).unwrap();
    let italic_meta = op_editor_core::runtime_font::parse(&italic).unwrap();
    assert!(normal_meta.cyrillic && italic_meta.cyrillic);
    assert!(!normal_meta.italic && italic_meta.italic);
    assert_eq!(normal_meta.family, italic_meta.family);
    let mut fonts = RuntimeFonts::default();
    fonts.register(&request(&[&normal, &italic])).unwrap();
    let provider: skia_safe::FontMgr = jian_skia::bundled_fonts::imported_provider()
        .unwrap()
        .into();
    let mut surface = skia_safe::surfaces::raster_n32_premul((1280, 300)).unwrap();
    surface.canvas().clear(skia_safe::Color::WHITE);
    let paint = skia_safe::Paint::default();
    let mut widths = Vec::new();
    for (index, (weight, italic)) in [(400, false), (700, false), (400, true), (700, true)]
        .into_iter()
        .enumerate()
    {
        let style = skia_safe::FontStyle::new(
            skia_safe::font_style::Weight::from(weight),
            skia_safe::font_style::Width::NORMAL,
            if italic {
                skia_safe::font_style::Slant::Italic
            } else {
                skia_safe::font_style::Slant::Upright
            },
        );
        let tf = provider
            .match_family_style(&normal_meta.family, style)
            .unwrap();
        assert_eq!(*tf.font_style().weight(), weight);
        assert_eq!(tf.is_italic(), italic);
        for ch in op_editor_core::runtime_font::RUSSIAN_ALPHABET.chars() {
            assert_ne!(tf.unichar_to_glyph(ch as i32), 0, "missing {ch}");
        }
        let font = skia_safe::Font::new(tf, 28.0);
        let text = "Русский текст: Съешь ещё этих мягких французских булок";
        widths.push(font.measure_str(text, None).0);
        surface
            .canvas()
            .draw_str(text, (20.0, 46.0 + index as f32 * 64.0), &font, &paint);
    }
    assert!((widths[0] - widths[1]).abs() > 1.0);
    assert!((widths[0] - widths[2]).abs() > 1.0);
    assert!((widths[2] - widths[3]).abs() > 1.0);
    let image = surface.image_snapshot();
    let png = image
        .encode(None, skia_safe::EncodedImageFormat::PNG, None)
        .unwrap();
    std::fs::write(
        "/private/tmp/openbrush-font-native-acceptance.png",
        png.as_bytes(),
    )
    .unwrap();
    println!("native Cyrillic normal400/700 italic400/700 widths: {widths:?}");
}

