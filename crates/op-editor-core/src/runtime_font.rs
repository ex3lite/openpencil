//! Byte-derived font identity shared by the native and CanvasKit runtimes.

use serde::{Deserialize, Serialize};

pub const MAX_FONT_BYTES: usize = 16 * 1024 * 1024;
pub const RUSSIAN_ALPHABET: &str =
    "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюя";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FontMetadata {
    pub family: String,
    pub weight: u16,
    pub style: String,
    pub italic: bool,
    pub weight_min: f32,
    pub weight_max: f32,
    pub variable: bool,
    pub cyrillic: bool,
    pub missing_cyrillic: String,
    /// Byte-derived offset of the fvar wght axis record for diagnostics.
    /// Rendering uses real Skia/CanvasKit variation coordinates.
    pub weight_axis_offset: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontParseError {
    TooLarge,
    InvalidFont,
    MissingFamily,
}

impl std::fmt::Display for FontParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooLarge => "font exceeds 16 MiB",
            Self::InvalidFont => "expected a single-face sfnt TTF/OTF font",
            Self::MissingFamily => "font has no intrinsic Unicode family name",
        })
    }
}
impl std::error::Error for FontParseError {}

pub fn parse(bytes: &[u8]) -> Result<FontMetadata, FontParseError> {
    if bytes.len() > MAX_FONT_BYTES {
        return Err(FontParseError::TooLarge);
    }
    // Collections/WOFF need an explicit face/decompression contract; accepting
    // them as face zero would silently discard other styles.
    if !matches!(bytes.get(..4), Some(b"\0\x01\0\0" | b"OTTO")) {
        return Err(FontParseError::InvalidFont);
    }
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| FontParseError::InvalidFont)?;
    let family = [16, 1]
        .into_iter()
        .find_map(|id| {
            face.names().into_iter().find_map(|name| {
                (name.name_id == id)
                    .then(|| name.to_string())
                    .flatten()
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
            })
        })
        .ok_or(FontParseError::MissingFamily)?;
    let weight = face.weight().to_number();
    let axis = face
        .variation_axes()
        .into_iter()
        .find(|axis| axis.tag == ttf_parser::Tag::from_bytes(b"wght"));
    let missing_cyrillic: String = RUSSIAN_ALPHABET
        .chars()
        .filter(|ch| face.glyph_index(*ch).is_none_or(|glyph| glyph.0 == 0))
        .collect();
    Ok(FontMetadata {
        family,
        weight,
        style: if face.is_italic() { "italic" } else { "normal" }.into(),
        italic: face.is_italic(),
        weight_min: axis.map_or(f32::from(weight), |axis| axis.min_value),
        weight_max: axis.map_or(f32::from(weight), |axis| axis.max_value),
        variable: axis.is_some(),
        cyrillic: missing_cyrillic.is_empty(),
        missing_cyrillic,
        weight_axis_offset: weight_axis_offset(bytes),
    })
}

fn weight_axis_offset(bytes: &[u8]) -> Option<usize> {
    let u16_at = |at| Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize);
    let u32_at = |at| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize);
    for i in 0..u16_at(4)? {
        let at = 12 + i * 16;
        if bytes.get(at..at + 4)? != b"fvar" {
            continue;
        }
        let start = u32_at(at + 8)?;
        let end = start.checked_add(u32_at(at + 12)?)?;
        bytes.get(start..end)?;
        let axes = start.checked_add(u16_at(start + 4)?)?;
        let count = u16_at(start + 8)?;
        let size = u16_at(start + 10)?;
        if size < 20 {
            return None;
        }
        for i in 0..count {
            let axis = axes.checked_add(i.checked_mul(size)?)?;
            if axis.checked_add(20)? > end {
                return None;
            }
            if bytes.get(axis..axis + 4)? == b"wght" {
                return Some(axis);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    const INTER: &[u8] = include_bytes!("../../op-host-desktop/assets/fonts/Inter-VF.ttf");
    const SERIF: &[u8] =
        include_bytes!("../../op-host-desktop/assets/fonts/InstrumentSerif-Regular.ttf");

    #[test]
    fn identity_ranges_and_russian_coverage_are_read_from_bytes() {
        let inter = parse(INTER).unwrap();
        assert_eq!(inter.family, "Inter");
        assert!(inter.variable && inter.weight_min <= 400.0 && inter.weight_max >= 700.0);
        assert!(inter.weight_axis_offset.is_some());
        // The bundled font may be a Latin subset: coverage must reflect it.
        let face = ttf_parser::Face::parse(INTER, 0).unwrap();
        assert_eq!(
            inter.cyrillic,
            RUSSIAN_ALPHABET
                .chars()
                .all(|c| face.glyph_index(c).is_some())
        );
        let serif = parse(SERIF).unwrap();
        assert!(!serif.variable);
        assert!(!serif.cyrillic);
        assert!(serif.missing_cyrillic.contains('Ё'));
    }

    #[test]
    fn rejects_non_fonts_and_oversize_before_parsing() {
        assert_eq!(parse(b"bad"), Err(FontParseError::InvalidFont));
        assert_eq!(
            parse(&vec![0; MAX_FONT_BYTES + 1]),
            Err(FontParseError::TooLarge)
        );
        assert_eq!(RUSSIAN_ALPHABET.chars().count(), 66);
    }
}

thread_local! { static GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
pub fn generation() -> u64 {
    GENERATION.with(|value| value.get())
}
pub fn invalidate_measurements() {
    GENERATION.with(|value| value.set(value.get().wrapping_add(1)));
}
