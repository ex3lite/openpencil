//! An inert source-model companion for standalone design HTML. The checksum
//! detects edits to the visible document. It is not a signature or trust check.

const START: &str =
    "<script type=\"application/json\" id=\"openbrush-source-v1\" data-html-checksum=\"";
const END: &str = "</script>";

fn checksum(parts: &[&str]) -> u64 {
    // Stable FNV-1a over the exact UTF-8 bytes on either side of the script.
    parts
        .iter()
        .flat_map(|part| part.bytes())
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
}

/// Embed the scoped design document (serialized JSON) without letting authored
/// text close the script element. The browser renders only the ordinary HTML.
pub fn embed_editable_document(html: &str, doc_json: &str) -> String {
    let at = html.rfind("</body>").unwrap_or(html.len());
    let (before, after) = html.split_at(at);
    format!(
        "{before}{START}{:016x}\">{}{END}{after}",
        checksum(&[html]),
        doc_json.replace('<', "\\u003c")
    )
}

/// The original markup can be checked directly or with only author-edited
/// plain text reverted. A caller must verify the checksum before restoration.
pub struct EditableHtml<'a> {
    pub markup: &'a str,
    pub document: &'a str,
    suffix: &'a str,
    expected: u64,
}

impl EditableHtml<'_> {
    pub fn matches_markup(&self, markup: &str) -> bool {
        checksum(&[markup, self.suffix]) == self.expected
    }
}

pub fn editable_source(html: &str) -> Option<EditableHtml<'_>> {
    let (markup, rest) = html.split_once(START)?;
    let (expected, rest) = rest.split_once("\">")?;
    let (document, suffix) = rest.split_once(END)?;
    if suffix.contains(START) || expected.len() != 16 {
        return None;
    }
    Some(EditableHtml {
        markup,
        document,
        suffix,
        expected: u64::from_str_radix(expected, 16).ok()?,
    })
}

/// Return the scoped model only for byte-exact exported markup.
pub fn unchanged_document_source(html: &str) -> Option<&str> {
    let source = editable_source(html)?;
    source
        .matches_markup(source.markup)
        .then_some(source.document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_text_cannot_end_inert_script_and_outer_edits_invalidate_source() {
        let html = "<html><body><p>Panel</p></body></html>";
        let model = r#"{"version":"1","name":"</script><b>inert</b>"}"#;
        let exported = embed_editable_document(html, model);
        assert_eq!(exported.matches(END).count(), 1);
        assert_eq!(
            unchanged_document_source(&exported),
            Some(r#"{"version":"1","name":"\u003c/script>\u003cb>inert\u003c/b>"}"#)
        );
        for edit in [
            exported.replace("<p>Panel", "<p>New"),
            exported.replace("</body>", "<aside></aside></body>"),
            format!("{exported}{exported}"),
        ] {
            assert!(unchanged_document_source(&edit).is_none());
        }
    }
}
