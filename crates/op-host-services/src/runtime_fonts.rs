//! Managed daemon font bridge. Receives bytes, never paths or remote URLs.

use base64::Engine;
use op_editor_core::runtime_font::{FontMetadata, MAX_FONT_BYTES};
use serde::Deserialize;
use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

pub(crate) const MAX_BODY: usize = 44 * 1024 * 1024;
const MAX_BATCH_BYTES: usize = 32 * 1024 * 1024;
const MAX_SESSION_BYTES: usize = 128 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct RuntimeFonts {
    faces: BTreeMap<String, FontEntry>,
    generation: u64,
    byte_length: usize,
    browser_generation: u64,
    browser_loaded: std::collections::BTreeSet<String>,
}

struct FontEntry {
    metadata: FontMetadata,
    bytes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    fonts: Vec<FontInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FontInput {
    data_base64: String,
}

#[derive(Debug)]
pub(crate) enum RegisterError {
    Invalid(String),
    Limit,
    NativeRegistration,
}
impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Limit => f.write_str("font byte/count limit exceeded"),
            Self::NativeRegistration => f.write_str("native renderer rejected font bytes"),
        }
    }
}
impl std::error::Error for RegisterError {}

impl RuntimeFonts {
    pub(crate) fn manifest(&self) -> String {
        let faces: Vec<_> = self
            .faces
            .iter()
            .map(|(hash, entry)| {
                let mut value =
                    serde_json::to_value(&entry.metadata).expect("font metadata serializes");
                value["sha256"] = hash.clone().into();
                value["byteLength"] = entry.bytes.len().into();
                value["nativeRegistered"] = true.into();
                value
            })
            .collect();
        serde_json::json!({"ok":true,"generation":self.generation,"faces":faces,
            "browser":{"generation":self.browser_generation,"loadedSha256":self.browser_loaded,
            "ready":!self.faces.is_empty() && self.browser_generation == self.generation && self.faces.keys().all(|key| self.browser_loaded.contains(key))}}).to_string()
    }

    pub(crate) fn note_browser_readiness(&mut self, body: &str) -> Result<(), RegisterError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Report {
            generation: u64,
            loaded_sha256: Vec<String>,
        }
        if body.len() > 64 * 1024 {
            return Err(RegisterError::Limit);
        }
        let report: Report = serde_json::from_str(body)
            .map_err(|_| RegisterError::Invalid("invalid font readiness report".into()))?;
        if report.generation != self.generation || report.loaded_sha256.len() > 512 {
            return Err(RegisterError::Invalid(
                "stale font readiness generation".into(),
            ));
        }
        if !report
            .loaded_sha256
            .iter()
            .all(|hash| self.faces.contains_key(hash))
        {
            return Err(RegisterError::Invalid("unknown font readiness hash".into()));
        }
        self.browser_generation = report.generation;
        self.browser_loaded = report.loaded_sha256.into_iter().collect();
        Ok(())
    }

    pub(crate) fn bytes(&self, path: &str) -> Option<&[u8]> {
        let hash = path.strip_prefix("/api/mcp/fonts/")?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        self.faces.get(hash).map(|entry| entry.bytes.as_slice())
    }

    pub(crate) fn register(&mut self, body: &str) -> Result<(), RegisterError> {
        if body.len() > MAX_BODY {
            return Err(RegisterError::Limit);
        }
        let request: Request = serde_json::from_str(body)
            .map_err(|_| RegisterError::Invalid("expected {fonts:[{dataBase64}]}".into()))?;
        if request.fonts.is_empty() || request.fonts.len() > 8 {
            return Err(RegisterError::Limit);
        }
        let mut pending = BTreeMap::new();
        let mut batch_bytes = 0usize;
        for input in request.fonts {
            if input.data_base64.len() > MAX_FONT_BYTES.div_ceil(3) * 4 {
                return Err(RegisterError::Limit);
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&input.data_base64)
                .map_err(|_| RegisterError::Invalid("invalid font base64".into()))?;
            batch_bytes = batch_bytes.saturating_add(bytes.len());
            if batch_bytes > MAX_BATCH_BYTES {
                return Err(RegisterError::Limit);
            }
            let metadata = op_editor_core::runtime_font::parse(&bytes)
                .map_err(|e| RegisterError::Invalid(e.to_string()))?;
            // Native and browser must address the same intrinsic family.
            let native = jian_skia::parse_imported_font_meta(&bytes)
                .ok_or(RegisterError::NativeRegistration)?;
            if native.family != metadata.family {
                return Err(RegisterError::Invalid(
                    "native and sfnt family names disagree".into(),
                ));
            }
            let hash = format!("{:x}", Sha256::digest(&bytes));
            if !self.faces.contains_key(&hash) {
                pending.insert(hash, FontEntry { metadata, bytes });
            }
        }
        if self.faces.len() + pending.len() > 512 {
            return Err(RegisterError::Limit);
        }
        let added: usize = pending.values().map(|entry| entry.bytes.len()).sum();
        if self.byte_length.saturating_add(added) > MAX_SESSION_BYTES {
            return Err(RegisterError::Limit);
        }
        // All parse/size checks precede mutations. Native registration reparses
        // exactly the validated bytes and bumps its measurement generation.
        for (hash, entry) in pending {
            jian_skia::register_imported_font(entry.bytes.clone())
                .map_err(|_| RegisterError::NativeRegistration)?;
            self.byte_length += entry.bytes.len();
            self.faces.insert(hash, entry);
            self.generation += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "runtime_fonts_tests.rs"]
mod tests;
