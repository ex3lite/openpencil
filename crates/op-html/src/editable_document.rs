//! Restore design-only source carried by unmodified standalone exports.
//! Changed markup goes through the same DOM/CSS importer as author-written HTML.

use std::collections::HashSet;

use jian_ops_schema::document::PenDocument;
use jian_ops_schema::node::PenNode;
use serde_json::{json, Value};

use crate::{HtmlImportResult, MAX_OUTPUT_NODES};

#[path = "editable_document_text.rs"]
mod text;

pub(crate) fn restore(source: &str) -> Option<HtmlImportResult> {
    let envelope = op_util::editable_html::editable_source(source)?;
    let json = envelope.document;
    if json.len() > op_util::document_limits::MAX_DOCUMENT_BYTES {
        return None;
    }
    let mut raw: Value = serde_json::from_str(json).ok()?;
    let mut ids = HashSet::new();
    let mut budget = MAX_OUTPUT_NODES - 1;
    // Only design data from the selection can be imported. In particular,
    // an HTML import is not a way to install application event handlers.
    let root = raw.as_object_mut()?;
    root.retain(|k, _| {
        matches!(
            k.as_str(),
            "version" | "children" | "themes" | "variables" | "pages"
        )
    });
    visit_nodes(root.get_mut("children")?, &mut ids, &mut budget, 0)?;
    if let Some(pages) = root.get_mut("pages").and_then(Value::as_array_mut) {
        for page in pages {
            visit_nodes(page.get_mut("children")?, &mut ids, &mut budget, 0)?;
        }
    }
    if !envelope.matches_markup(envelope.markup) {
        text::reconcile(&mut raw, &envelope)?;
    }
    let mut doc: PenDocument = serde_json::from_value(raw).ok()?;
    if doc.children.is_empty() {
        return None;
    }
    let masters: Vec<_> = doc
        .pages
        .take()
        .into_iter()
        .flatten()
        .flat_map(|page| page.children)
        .collect();
    if !masters.is_empty() {
        let mut library_id = "html_components".to_string();
        while ids.contains(&library_id) {
            library_id.push('_');
        }
        let mut library = serde_json::from_value::<PenNode>(json!({
            "type":"frame", "id":library_id,"name":"Imported components","visible":false
        }))
        .ok()?;
        if let PenNode::Frame(frame) = &mut library {
            frame.children = Some(masters);
        }
        doc.children.push(library);
    }
    let mut result = HtmlImportResult::new(doc.children, Vec::new());
    result.variables = doc.variables.unwrap_or_default();
    result.themes = doc.themes.unwrap_or_default();
    Some(result)
}

fn visit_nodes(
    nodes: &mut Value,
    ids: &mut HashSet<String>,
    budget: &mut usize,
    depth: usize,
) -> Option<()> {
    if depth >= crate::dom::MAX_DOM_DEPTH {
        return None;
    }
    for node in nodes.as_array_mut()? {
        *budget = budget.checked_sub(1)?;
        let object = node.as_object_mut()?;
        let id = object.get("id")?.as_str()?;
        if id.is_empty() || !ids.insert(id.to_string()) {
            return None;
        }
        strip_actions(object);
        if let Some(children) = object.get_mut("children") {
            visit_nodes(children, ids, budget, depth + 1)?;
        }
    }
    Some(())
}

fn strip_actions(object: &mut serde_json::Map<String, Value>) {
    for key in [
        "events",
        "lifecycle",
        "gestures",
        "route",
        "state",
        "bindings",
    ] {
        object.remove(key);
    }
    // Nested component overrides carry partial nodes, including their own
    // descendant overrides. Apply the same import contract at each level.
    if let Some(overrides) = object.get_mut("descendants").and_then(Value::as_object_mut) {
        for props in overrides.values_mut().filter_map(Value::as_object_mut) {
            strip_actions(props);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn edited_markup_reads_visible_text_and_vector_instead_of_stale_source() {
        use serde_json::json;
        let original = r##"<body><section style="display:flex;flex-direction:column;gap:11px"><span>Before HTML edit</span><svg data-op-id="icon" preserveAspectRatio="none" viewBox="0 0 12 12" style="width:12px;height:12px"><path d="M0 0L10 10" fill="none" stroke="#123456" stroke-width="2"/></svg></section></body>"##;
        let model = json!({"version":"1","children":[{"type":"text","id":"old","content":"Stale metadata"}]});
        let html = op_util::editable_html::embed_editable_document(original, &model.to_string())
            .replace("Before HTML edit", "Live HTML edit")
            .replace("M0 0L10 10", "M0 0L12 12");
        let result = crate::import_html(&html, &Default::default());
        let value = serde_json::to_string(&result.nodes).unwrap();
        assert!(value.contains("Live HTML edit"));
        assert!(value.contains("M0 0L12 12"));
        assert!(value.contains(r#""type":"path""#));
        assert!(value.contains(r#""gap":11"#));
        assert!(!value.contains("Stale metadata"));
        assert!(!value.contains("data:image"));
    }
}
