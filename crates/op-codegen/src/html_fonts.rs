//! Ship the same font faces as the editor with a standalone HTML export.
//! Each face is a WOFF2 encoding of its bundled TTF, retaining all glyphs and axes.
use crate::{node_children, root_nodes};
use base64::{engine::general_purpose::STANDARD, Engine};
use jian_ops_schema::node::{PenNode, TextContent};
use jian_ops_schema::PenDocument;
use std::collections::BTreeSet;
use std::fmt::Write;

struct Face {
    family: &'static str,
    weight: &'static str,
    #[cfg(not(target_arch = "wasm32"))]
    bytes: &'static [u8],
    #[cfg(target_arch = "wasm32")]
    file: &'static str,
}

macro_rules! face {
    ($family:literal, $weight:literal, $file:literal) => {
        Face {
            family: $family,
            weight: $weight,
            #[cfg(target_arch = "wasm32")]
            file: concat!($file, ".ttf"),
            #[cfg(not(target_arch = "wasm32"))]
            bytes: include_bytes!(concat!("../assets/webfonts/", $file, ".woff2")),
        }
    };
}

const FACES: &[Face] = &[
    face!("Inter", "100 900", "Inter-VF"),
    face!("Roboto", "400", "Roboto-Regular"),
    face!("DM Sans", "100 1000", "DMSans-VF"),
    face!("DM Mono", "400", "DMMono-Regular"),
    face!("DM Mono", "500", "DMMono-Medium"),
    face!("DM Serif Display", "400", "DMSerifDisplay-Regular"),
    face!("Outfit", "100 900", "Outfit-VF"),
    face!("JetBrains Mono", "100 800", "JetBrainsMono-VF"),
    face!("Manrope", "200 800", "Manrope-VF"),
    face!("Space Grotesk", "300 700", "SpaceGrotesk-VF"),
    face!("Instrument Serif", "400", "InstrumentSerif-Regular"),
    face!("Cormorant Garamond", "300 700", "CormorantGaramond-VF"),
];

fn collect<'a>(nodes: &'a [PenNode], families: &mut BTreeSet<&'a str>) {
    for node in nodes {
        if let PenNode::Text(text) = node {
            families.extend(text.font_family.as_deref());
            if let TextContent::Styled(runs) = &text.content {
                families.extend(runs.iter().filter_map(|run| run.font_family.as_deref()));
            }
        }
        collect(node_children(node), families);
    }
}

pub(crate) fn stylesheet(doc: &PenDocument) -> String {
    let mut families = BTreeSet::new();
    collect(root_nodes(doc), &mut families);
    let mut out = String::new();
    for face in FACES {
        if families
            .iter()
            .any(|name| name.eq_ignore_ascii_case(face.family))
        {
            #[cfg(not(target_arch = "wasm32"))]
            append_face(&mut out, face, face.bytes, "woff2", "woff2");
            #[cfg(target_arch = "wasm32")]
            WEB_FACES.with_borrow(|faces| {
                if let Some(data) = faces.get(face.file) {
                    append_face(&mut out, face, data, "ttf", "truetype");
                }
            });
        }
    }
    out
}

fn append_face(out: &mut String, face: &Face, bytes: &[u8], mime: &str, format: &str) {
    let _ = write!(out, "@font-face{{font-family:'{}';font-weight:{};font-style:normal;src:url(data:font/{mime};base64,", face.family, face.weight);
    STANDARD.encode_string(bytes, out);
    let _ = writeln!(out, ") format('{format}')}}");
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WEB_FACES: std::cell::RefCell<std::collections::BTreeMap<String, Vec<u8>>> = const {
        std::cell::RefCell::new(std::collections::BTreeMap::new())
    };
}

/// Reuse the font already fetched by the browser editor. Keeping font files
/// out of the editor WASM lets it start before its font assets arrive.
#[cfg(target_arch = "wasm32")]
pub fn register_bundled_export_font(file: &str, bytes: Vec<u8>) {
    if FACES.iter().any(|face| face.file == file) {
        WEB_FACES.with_borrow_mut(|faces| faces.insert(file.to_owned(), bytes));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn standalone_html_embeds_only_used_faces_once_including_rich_text() {
        let doc = serde_json::from_value(serde_json::json!({
            "version": "1", "children": [{"type":"frame","id":"row","children":[
                {"type":"text","id":"a","content":"Проект","fontFamily":"Inter"},
                {"type":"text","id":"b","fontFamily":"Inter",
                 "content":[{"text":"Слои","fontFamily":"Manrope"}]}
            ]}]
        }))
        .unwrap();
        let css = stylesheet(&doc);
        assert_eq!(css.matches("@font-face").count(), 2);
        assert!(css.contains("font-family:'Inter'"));
        assert!(css.contains("font-family:'Manrope'"));
        assert_eq!(css.matches("data:font/woff2;base64,d09GMg").count(), 2);
    }
}
