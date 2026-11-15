//! Optional local resource packs. Documents stay outside the binary so a host
//! can install licensed vendor kits without redistributing them in the fork.
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

use crate::{user_scene_templates as templates, web_assets as assets};

const ROOT: &str = "/pkg/assets/resource-library/";
const CATALOG: &str = "/pkg/assets/resource-library/catalog.json";
const MAX_DOCUMENT: usize = 12 * 1024 * 1024;

#[derive(Deserialize)]
struct Catalog {
    version: u8,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    name: String,
    document: String,
    preview: String,
    frames: u16,
    frame_width: u32,
    frame_height: u32,
}

#[derive(Default)]
struct Library {
    parsed: bool,
    entries: Vec<Entry>,
    finished: HashSet<String>,
}

fn library() -> &'static Mutex<Library> {
    static LIBRARY: OnceLock<Mutex<Library>> = OnceLock::new();
    LIBRARY.get_or_init(|| Mutex::new(Library::default()))
}

fn basename(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && !value.contains("..")
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

fn parse(bytes: &[u8]) -> Option<Vec<Entry>> {
    if bytes.len() > 256 * 1024 {
        return None;
    }
    let catalog: Catalog = serde_json::from_slice(bytes).ok()?;
    if catalog.version != 1 || catalog.entries.len() > 24 {
        return None;
    }
    let mut ids = HashSet::new();
    for entry in &catalog.entries {
        if !entry.id.starts_with("library-")
            || !basename(&entry.id)
            || !ids.insert(&entry.id)
            || !basename(&entry.document)
            || !entry.document.ends_with(".op")
            || !basename(&entry.preview)
            || !entry.preview.ends_with(".jpg")
            || entry.name.is_empty()
            || entry.name.len() > 512
            || entry.name.chars().any(char::is_control)
            || entry.frames == 0
            || entry.frame_width == 0
            || entry.frame_height == 0
        {
            return None;
        }
    }
    Some(catalog.entries)
}

fn materialize(
    entry: &Entry,
    bytes: &[u8],
    preview: &[u8],
) -> Option<templates::UserSceneTemplate> {
    if bytes.len() > MAX_DOCUMENT
        || preview.len() > 2 * 1024 * 1024
        || !preview.starts_with(&[0xff, 0xd8, 0xff])
    {
        return None;
    }
    let document = std::str::from_utf8(bytes).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(document).ok()?;
    if !parsed.get("children")?.is_array() {
        return None;
    }
    Some(templates::UserSceneTemplate {
        id: format!("user:{}", entry.id),
        name: entry.name.clone(),
        frames: entry.frames,
        frame_width: entry.frame_width,
        frame_height: entry.frame_height,
        document: document.to_string(),
        preview_jpeg: preview.to_vec(),
    })
}

/// Called from the host repaint. Each route is requested once; missing packs
/// leave the original catalogue available. Registration never replaces a
/// user's own ids and never modifies the open document.
pub fn pump() -> bool {
    let mut library = library().lock().unwrap_or_else(|p| p.into_inner());
    if !library.parsed {
        match assets::state(CATALOG) {
            assets::WebAssetState::Absent => {
                assets::request(CATALOG);
                return false;
            }
            assets::WebAssetState::Pending => return false,
            assets::WebAssetState::Failed => {
                library.parsed = true;
                return false;
            }
            assets::WebAssetState::Ready => {
                library.entries = assets::installed_bytes(CATALOG)
                    .and_then(parse)
                    .unwrap_or_default();
                library.parsed = true;
            }
        }
    }
    let mut registered = false;
    // At most 24 small metadata entries. Assets themselves remain in the
    // existing single-flight registry and are cloned only on registration.
    let Library {
        entries, finished, ..
    } = &mut *library;
    for entry in entries {
        if finished.contains(&entry.id) {
            continue;
        }
        let document = format!("{ROOT}{}", entry.document);
        let preview = format!("{ROOT}{}", entry.preview);
        for route in [&document, &preview] {
            if assets::state(route) == assets::WebAssetState::Absent {
                assets::request(route);
            }
        }
        if let (Some(bytes), Some(jpeg)) = (
            assets::installed_bytes(&document),
            assets::installed_bytes(&preview),
        ) {
            if let Some(template) = materialize(entry, bytes, jpeg) {
                registered |= templates::load_user_scene_template(template).is_ok();
            }
            finished.insert(entry.id.clone());
        } else if [document, preview]
            .iter()
            .any(|r| assets::state(r) == assets::WebAssetState::Failed)
        {
            finished.insert(entry.id.clone());
        }
    }
    registered
}

/// Real installed resources take precedence over the legacy demo gallery.
pub fn available() -> bool {
    templates::user_scene_templates()
        .iter()
        .any(|t| t.id.starts_with("user:library-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(document: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"version":1,"entries":[{
            "id":"library-original", "name":"Original source", "document":document,
            "preview":"original.jpg", "frames":1, "frameWidth":400, "frameHeight":800
        }]}))
        .unwrap()
    }

    #[test]
    fn refuses_remote_routes_and_traversal() {
        for unsafe_path in [
            "../secret.op",
            "/secret.op",
            "https://host/source.op",
            "x%2fy.op",
            "x\\y.op",
        ] {
            assert!(parse(&catalog(unsafe_path)).is_none());
        }
        assert!(parse(&catalog("original.op")).is_some());
    }

    #[test]
    fn imports_source_bytes_without_replacing_authored_content() {
        let entry = parse(&catalog("original.op")).unwrap().remove(0);
        let bytes = br#"{"version":"0.8.5","children":[{"type":"text","content":"Original"}]}"#;
        let template = materialize(&entry, bytes, &[0xff, 0xd8, 0xff]).unwrap();
        assert_eq!(template.document.as_bytes(), bytes);
        assert_eq!(template.id, "user:library-original");
        assert!(materialize(
            &entry,
            br#"{"error":"not a document"}"#,
            &[0xff, 0xd8, 0xff]
        )
        .is_none());
        assert!(materialize(&entry, bytes, b"not a JPEG").is_none());
    }
}
