//! Reconcile a visible plain-text edit only when restoring its old characters
//! recovers the exact exported HTML. Structure, attributes, CSS and rich runs
//! otherwise follow the general DOM importer.

use std::collections::{HashMap, HashSet};

use op_util::{editable_html::EditableHtml, xml_escape::escape_html};
use serde_json::{Map, Value};

use crate::dom::{parse_dom, DomNode};

struct Authored<'a> {
    node: &'a Value,
    pointer: String,
    inside_master: bool,
}

fn index<'a>(
    nodes: &'a [Value],
    parent: &str,
    in_master: bool,
    all: &mut HashMap<&'a str, Authored<'a>>,
) -> Option<()> {
    for (i, node) in nodes.iter().enumerate() {
        let id = node.get("id")?.as_str()?;
        let pointer = format!("{parent}/{i}");
        let master = in_master || node.get("reusable").and_then(Value::as_bool) == Some(true);
        if let Some(children) = node.get("children").and_then(Value::as_array) {
            index(children, &format!("{pointer}/children"), master, all)?;
        }
        all.insert(
            id,
            Authored {
                node,
                pointer,
                inside_master: master,
            },
        );
    }
    Some(())
}

struct Edit<'a> {
    owner: &'a str,
    child: Option<&'a str>,
    original: &'a str,
}

fn resolve<'a>(id: &str, all: &'a HashMap<&str, Authored<'_>>) -> Option<Edit<'a>> {
    let mut found = None;
    if let Some(node) = all.get(id) {
        // Editing a visible master may affect a second, unchanged instance.
        // The DOM importer preserves their independently visible appearances.
        if node.inside_master
            || !node.pointer.starts_with("/children/")
            || node.node["type"] != "text"
        {
            return None;
        }
        found = Some(Edit {
            owner: &node.pointer,
            child: None,
            original: node.node["content"].as_str()?,
        });
    }
    // Expanded ids are the editor's instanceId__descendantId form. Require a
    // real descendant of this specific master and exactly one interpretation.
    for (at, _) in id.match_indices("__") {
        let (prefix, tail) = id.split_at(at);
        let child_id = &tail[2..];
        let Some(instance) = all.get(prefix) else {
            continue;
        };
        if instance.node["type"] != "ref"
            || !instance.pointer.starts_with("/children/")
            || instance.inside_master
        {
            continue;
        }
        let Some(master) = instance.node["ref"].as_str().and_then(|id| all.get(id)) else {
            continue;
        };
        let Some((child_key, child)) = all.get_key_value(child_id) else {
            continue;
        };
        if child.node["type"] != "text"
            || !child
                .pointer
                .strip_prefix(&master.pointer)
                .is_some_and(|tail| tail.starts_with("/children/"))
        {
            continue;
        }
        let original = instance
            .node
            .get("descendants")
            .and_then(|v| v.get(child_id))
            .and_then(|v| v.get("content"))
            .unwrap_or(&child.node["content"])
            .as_str()?;
        if found.is_some() {
            return None;
        }
        found = Some(Edit {
            owner: &instance.pointer,
            child: Some(*child_key),
            original,
        });
    }
    found
}

fn decoded(raw: &str) -> Option<String> {
    if !raw.contains(['&', '\r', '\0']) {
        return Some(raw.to_string());
    }
    // Decode HTML character references and newlines as the document parser does.
    let dom = parse_dom(&format!("<span>{raw}</span>"));
    let [DomNode::Element(span)] = dom.body.as_slice() else {
        return None;
    };
    match span.children.as_slice() {
        [] => Some(String::new()),
        [DomNode::Text(t)] => Some(t.clone()),
        _ => None,
    }
}

pub(super) fn reconcile(doc: &mut Value, envelope: &EditableHtml<'_>) -> Option<()> {
    let mut all = HashMap::new();
    index(doc["children"].as_array()?, "/children", false, &mut all)?;
    if let Some(pages) = doc.get("pages").and_then(Value::as_array) {
        for (i, page) in pages.iter().enumerate() {
            index(
                page["children"].as_array()?,
                &format!("/pages/{i}/children"),
                false,
                &mut all,
            )?;
        }
    }

    // This recognizes exactly the writer's plain <span data-op-id="…"> body.
    // Use the same escaping to reconstruct the pre-edit serialized HTML. No
    // other byte may differ for the source model to be used.
    const SPAN: &str = "<span data-op-id=\"";
    let html = envelope.markup;
    let mut cursor = 0;
    let mut previous = 0;
    let mut baseline = String::with_capacity(html.len());
    let mut edits = Vec::new();
    let mut seen = HashSet::new();
    while let Some(next) = html[cursor..].find(SPAN) {
        let id_at = cursor + next + SPAN.len();
        let (raw_id, attrs) = html[id_at..].split_once('"')?;
        let start = html.len() - attrs.len() + attrs.find('>')? + 1;
        cursor = start;
        let end = start + html[start..].find('<')?;
        if !html[end..].starts_with("</span>") {
            continue;
        }
        cursor = end + "</span>".len();
        let Some(edit) = resolve(&decoded(raw_id)?, &all) else {
            continue;
        };
        let raw = &html[start..end];
        let original = escape_html(edit.original);
        if !seen.insert((edit.owner, edit.child)) {
            return None;
        }
        if raw == original {
            continue;
        }
        baseline.push_str(&html[previous..start]);
        baseline.push_str(&original);
        previous = end;
        edits.push((
            edit.owner.to_owned(),
            edit.child.map(str::to_owned),
            decoded(raw)?,
        ));
    }
    if edits.is_empty() {
        return None;
    }
    baseline.push_str(&html[previous..]);
    if !envelope.matches_markup(&baseline) {
        return None;
    }

    for (owner, child, text) in edits {
        let node = doc.pointer_mut(&owner)?.as_object_mut()?;
        let target = if let Some(child) = child {
            let overrides = node
                .entry("descendants")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()?;
            overrides
                .entry(child)
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()?
        } else {
            node
        };
        target.insert("content".into(), Value::String(text));
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use op_util::editable_html::embed_editable_document;
    use serde_json::json;

    #[test]
    fn visible_text_changes_update_local_text_and_instance_override_only() {
        let doc = json!({
            "version":"1","variables":{"ink":{"type":"color","value":"#111111"}},
            "children":[{"type":"frame","id":"panel","name":"Panel","layout":"horizontal","gap":13,"children":[
                {"type":"text","id":"brand","content":"Brand & co."},
                {"type":"ref","id":"instance","ref":"button","descendants":{"label":{"fontSize":14}}},
                {"type":"ref","id":"another","ref":"button"}
            ]}],
            "pages":[{"id":"components","name":"Components","children":[
                {"type":"frame","id":"button","reusable":true,"children":[
                    {"type":"text","id":"label","content":"Default label"}
                ]}
            ]}]
        });
        let markup = r#"<body><span data-op-id="brand">Brand &amp; co.</span><button><span data-op-id="instance__label">Default label</span></button><button><span data-op-id="another__label">Default label</span></button></body>"#;
        let html = embed_editable_document(markup, &doc.to_string());
        let updated = html
            .replacen(">Brand &amp; co.</span>", ">R&amp;D &lt;team&gt;</span>", 1)
            .replacen(">Default label</span>", ">Изменённая кнопка</span>", 1);
        let restored = crate::import_html_document(&updated, &Default::default(), None, None);
        let data = serde_json::to_value(restored.document).unwrap();
        assert_eq!(data["children"][0]["gap"], 13.);
        assert_eq!(data["children"][0]["children"][0]["content"], "R&D <team>");
        let instance = &data["children"][0]["children"][1];
        assert_eq!(instance["ref"], "button");
        assert_eq!(
            instance["descendants"]["label"]["content"],
            "Изменённая кнопка"
        );
        assert_eq!(instance["descendants"]["label"]["fontSize"], 14);
        assert!(data["children"][0]["children"][2]
            .get("descendants")
            .is_none());
        assert_eq!(
            data["children"][1]["children"][0]["children"][0]["content"],
            "Default label"
        );
        assert!(data["variables"].get("ink").is_some());

        for other in [
            updated.replacen("<button>", "<button style=\"background:#aabbcc\">", 1),
            updated.replacen("Изменённая кнопка</span>", "<b>Rich change</b></span>", 1),
        ] {
            let imported = crate::import_html(&other, &Default::default());
            assert!(imported.variables.is_empty());
            assert!(serde_json::to_string(&imported.nodes)
                .unwrap()
                .contains("R&D"));
        }
    }
}
