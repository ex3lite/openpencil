//! Authored selection and render input for standalone HTML generation.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use jian_ops_schema::variable::{VariableDefinition, VariableScalar, VariableValue};
use jian_ops_schema::{node::PenNode, page::PenPage, PenDocument};
use op_editor_core::pen_node_ext::PenNodeExt;
use op_editor_core::{ref_resolve, variables_resolve};
use serde_json::Value;

use crate::Html;

/// A selected forest plus the master definitions and design tokens it uses.
/// Owned so a Code panel run can survive edits to the live document.
#[derive(Clone, Debug)]
pub struct HtmlSelection {
    source: PenDocument,
    active_theme: BTreeMap<String, String>,
}

impl Html {
    pub fn generate_selection<'a>(
        &self,
        doc: &'a PenDocument,
        roots: impl IntoIterator<Item = &'a PenNode>,
        active_theme: &BTreeMap<String, String>,
    ) -> String {
        HtmlSelection::new(doc, roots, active_theme).generate()
    }
}

impl HtmlSelection {
    pub fn new<'a>(
        doc: &'a PenDocument,
        roots: impl IntoIterator<Item = &'a PenNode>,
        active_theme: &BTreeMap<String, String>,
    ) -> Self {
        let selected: HashSet<&str> = roots
            .into_iter()
            .map(|node| node.base().id.as_str())
            .collect();
        let forest: Vec<&[PenNode]> = doc
            .pages
            .iter()
            .flatten()
            .map(|p| p.children.as_slice())
            .chain(std::iter::once(doc.children.as_slice()))
            .collect();
        // Selection order can be click order, and a selected frame may include another
        // selected node. Traverse the original forest once to retain paint order.
        let roots = ordered_roots(&forest, &selected);
        let mut available = HashMap::new();
        let mut pending = BTreeSet::new();
        let mut tokens = BTreeSet::new();
        let mut included = HashSet::new();
        for node in &roots {
            walk(std::slice::from_ref(*node), &mut |node| {
                included.insert(node.base().id.as_str());
            });
            collect_dependencies(node, &mut pending, &mut tokens);
        }
        let mut masters = HashSet::new();
        if !pending.is_empty() {
            for nodes in &forest {
                walk(nodes, &mut |node| {
                    available.entry(node.base().id.as_str()).or_insert(node);
                });
            }
            while let Some(target) = pending.pop_first() {
                let Some(master) = available.get(target.as_str()).copied() else {
                    continue;
                };
                if included.contains(master.base().id.as_str())
                    || !masters.insert(master.base().id.as_str())
                {
                    continue;
                }
                collect_dependencies(master, &mut pending, &mut tokens);
            }
        }
        let mut source = empty_document(doc);
        // A standalone design has no host UI state. Make the exported theme
        // its default while keeping all authored modes available on import.
        if let Some(axes) = &mut source.themes {
            for (axis, value) in active_theme {
                if let Some(values) = axes.get_mut(axis) {
                    if let Some(index) = values.iter().position(|v| v == value) {
                        values[..=index].rotate_right(1);
                    }
                }
            }
        }
        source.children = roots.into_iter().cloned().collect();
        let dependencies: Vec<_> = ordered_roots(&forest, &masters)
            .into_iter()
            .cloned()
            .collect();
        if !dependencies.is_empty() {
            source.pages = Some(vec![PenPage {
                id: "openbrush-html-components".into(),
                name: "Components".into(),
                children: dependencies,
                background_color: None,
                state: None,
                lifecycle: None,
            }]);
        }
        if let Some(definitions) = &doc.variables {
            let mut variables = BTreeMap::new();
            while let Some(name) = tokens.pop_first() {
                if variables.contains_key(&name) {
                    continue;
                }
                if let Some(definition) = definitions.get(&name) {
                    collect_variable_aliases(definition, &mut tokens);
                    variables.insert(name, definition.clone());
                }
            }
            if !variables.is_empty() {
                source.variables = Some(variables);
            }
        }
        Self {
            source,
            active_theme: active_theme.clone(),
        }
    }

    pub fn prepared_document(&self) -> PenDocument {
        self.prepare(false)
    }

    fn prepare(&self, live_css: bool) -> PenDocument {
        let mut prepared = empty_document(&self.source);
        prepared.children = if ref_resolve::roots_have_refs(&self.source.children) {
            ref_resolve::resolve_refs_for_canvas_roots(&self.source.children, &self.source)
        } else {
            self.source.children.clone()
        };
        let theme = variables_resolve::effective_theme(&self.source, &self.active_theme);
        let variables = self.source.variables.as_ref();
        let css_variables = if live_css {
            self.css_variables(&theme)
        } else {
            BTreeMap::new()
        };
        for root in &mut prepared.children {
            // Keep the renderer-backed values for text and all fallback cases.
            // Restore authored bindings only where HTML supports a corresponding CSS property.
            let bindings = if css_variables.is_empty() {
                Vec::new()
            } else {
                css_bindings(root, &css_variables)
            };
            resolve_sizes(root, variables, &theme);
            variables_resolve::resolve_node_for_canvas(root, variables, &theme);
            if !bindings.is_empty() {
                if let Ok(mut value) = serde_json::to_value(&*root) {
                    for (path, token) in bindings {
                        if let Some(slot) = value.pointer_mut(&path) {
                            *slot = Value::String(token);
                        }
                    }
                    if let Ok(bound) = serde_json::from_value(value) {
                        *root = bound;
                    }
                }
            }
        }
        if !css_variables.is_empty() {
            prepared.variables = Some(css_variables);
        }
        prepared
    }

    fn css_variables(
        &self,
        theme: &BTreeMap<String, String>,
    ) -> BTreeMap<String, VariableDefinition> {
        let mut result = BTreeMap::new();
        if let Some(definitions) = &self.source.variables {
            for (name, definition) in definitions {
                let reference = format!("${name}");
                let Some(resolved) =
                    variables_resolve::resolve_variable_ref(&reference, Some(definitions), theme)
                else {
                    continue;
                };
                let value = match variables_resolve::themed_scalar(definition, theme) {
                    Some(VariableScalar::Str(alias))
                        if alias
                            .strip_prefix('$')
                            .is_some_and(|key| definitions.contains_key(key)) =>
                    {
                        VariableScalar::Str(alias.clone())
                    }
                    _ => resolved,
                };
                result.insert(
                    name.clone(),
                    VariableDefinition {
                        kind: definition.kind.clone(),
                        value: VariableValue::Scalar(value),
                    },
                );
            }
        }
        result
    }

    pub fn generate(&self) -> String {
        let html = Html.generate_prepared(&self.prepare(true));
        match self.editable_source_json() {
            Ok(json) => op_util::editable_html::embed_editable_document(&html, &json),
            Err(_) => html,
        }
    }

    fn editable_source_json(&self) -> serde_json::Result<String> {
        let mut master_ids = HashSet::new();
        for page in self.source.pages.iter().flatten() {
            walk(&page.children, &mut |node| {
                master_ids.insert(node.base().id.as_str());
            });
        }
        if master_ids.is_empty() {
            return serde_json::to_string(&self.source);
        }
        let mut taken = master_ids.clone();
        let mut overlapping = false;
        walk(&self.source.children, &mut |node| {
            overlapping |= !taken.insert(node.base().id.as_str());
        });
        if !overlapping {
            return serde_json::to_string(&self.source);
        }

        // Selecting a child of a master and a separate instance displays the
        // child twice. Keep the master subtree's original IDs and override
        // keys. Only its independently exported visible copy needs fresh IDs.
        fn rename_copy(
            nodes: &mut [PenNode],
            master_ids: &HashSet<&str>,
            taken: &mut HashSet<String>,
        ) {
            for node in nodes {
                let id = &mut node.base_mut().id;
                if master_ids.contains(id.as_str()) {
                    let mut fresh = format!("html-copy-{id}");
                    while taken.contains(&fresh) {
                        fresh.push('_');
                    }
                    taken.insert(fresh.clone());
                    *id = fresh;
                }
                if let Some(children) = node.children_mut() {
                    rename_copy(children, master_ids, taken);
                }
            }
        }
        let mut distinct = self.source.clone();
        let mut taken: HashSet<String> = taken.into_iter().map(str::to_owned).collect();
        rename_copy(&mut distinct.children, &master_ids, &mut taken);
        serde_json::to_string(&distinct)
    }

    pub fn variables(&self) -> Option<&BTreeMap<String, VariableDefinition>> {
        self.source.variables.as_ref()
    }

    pub fn node_count(&self) -> usize {
        self.source.children.len()
    }
}

fn resolve_sizes(
    node: &mut PenNode,
    vars: Option<&BTreeMap<String, VariableDefinition>>,
    theme: &BTreeMap<String, String>,
) {
    Html::resolve_sizing(node, vars, theme);
    if let Some(children) = node.children_mut() {
        for child in children {
            resolve_sizes(child, vars, theme);
        }
    }
}

fn css_bindings(
    node: &PenNode,
    vars: &BTreeMap<String, VariableDefinition>,
) -> Vec<(String, String)> {
    fn visit(
        value: &Value,
        path: &str,
        vars: &BTreeMap<String, VariableDefinition>,
        out: &mut Vec<(String, String)>,
    ) {
        match value {
            Value::Object(map) => {
                for (key, item) in map {
                    // Escape JSON Pointer segments (RFC 6901).
                    let field = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                    if matches!(
                        key.as_str(),
                        "color"
                            | "fill"
                            | "width"
                            | "height"
                            | "gap"
                            | "padding"
                            | "opacity"
                            | "fontWeight"
                    ) {
                        if let Some(token) = item
                            .as_str()
                            .filter(|s| s.strip_prefix('$').is_some_and(|n| vars.contains_key(n)))
                        {
                            out.push((field, token.into()));
                            continue;
                        }
                    }
                    if item.is_array() || item.is_object() {
                        visit(item, &field, vars, out);
                    }
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    visit(item, &format!("{path}/{index}"), vars, out);
                }
            }
            _ => {}
        }
    }
    let mut bindings = Vec::new();
    if let Ok(value) = serde_json::to_value(node) {
        visit(&value, "", vars, &mut bindings);
    }
    bindings
}

fn empty_document(doc: &PenDocument) -> PenDocument {
    PenDocument {
        version: doc.version.clone(),
        format_version: doc.format_version.clone(),
        themes: doc.themes.clone(),
        motion: doc.motion,
        responsive: doc.responsive,
        name: None,
        variables: None,
        children: Vec::new(),
        pages: None,
        id: None,
        app: None,
        routes: None,
        state: None,
        lifecycle: None,
        logic_modules: None,
        design_md: None,
        conversion: None,
    }
}

fn ordered_roots<'a>(forest: &[&'a [PenNode]], ids: &HashSet<&str>) -> Vec<&'a PenNode> {
    fn walk<'a>(nodes: &'a [PenNode], ids: &mut HashSet<&str>, roots: &mut Vec<&'a PenNode>) {
        for node in nodes {
            if ids.is_empty() {
                break;
            }
            if ids.remove(node.base().id.as_str()) {
                roots.push(node);
            } else if let Some(children) = node.children() {
                walk(children, ids, roots);
            }
        }
    }
    let mut roots = Vec::new();
    let mut remaining = ids.clone();
    for nodes in forest {
        walk(nodes, &mut remaining, &mut roots);
    }
    roots
}

fn walk<'a>(nodes: &'a [PenNode], visit: &mut impl FnMut(&'a PenNode)) {
    for node in nodes {
        visit(node);
        if let Some(children) = node.children() {
            walk(children, visit);
        }
    }
}

fn collect_dependencies(
    node: &PenNode,
    refs: &mut BTreeSet<String>,
    tokens: &mut BTreeSet<String>,
) {
    fn variable_names(value: &Value, tokens: &mut BTreeSet<String>) {
        match value {
            Value::String(s) => {
                if let Some(name) = s.strip_prefix('$') {
                    tokens.insert(name.into());
                }
            }
            Value::Array(items) => {
                for item in items {
                    variable_names(item, tokens);
                }
            }
            Value::Object(items) => {
                for item in items.values() {
                    variable_names(item, tokens);
                }
            }
            _ => {}
        }
    }
    fn override_refs(props: &Value, refs: &mut BTreeSet<String>) {
        if let Some(target) = props.get("ref").and_then(Value::as_str) {
            refs.insert(target.into());
        }
        if let Some(overrides) = props.get("descendants").and_then(Value::as_object) {
            for patch in overrides.values() {
                override_refs(patch, refs);
            }
        }
    }
    walk(std::slice::from_ref(node), &mut |child| {
        if let PenNode::Ref(instance) = child {
            refs.insert(instance.target.clone());
            if let Some(overrides) = &instance.descendants {
                for props in overrides.values() {
                    override_refs(props, refs);
                }
            }
        }
    });
    if let Ok(value) = serde_json::to_value(node) {
        variable_names(&value, tokens);
    }
}

fn collect_variable_aliases(def: &VariableDefinition, tokens: &mut BTreeSet<String>) {
    use jian_ops_schema::variable::{VariableScalar, VariableValue};
    let mut add = |value: &VariableScalar| {
        if let VariableScalar::Str(s) = value {
            if let Some(name) = s.strip_prefix('$') {
                tokens.insert(name.into());
            }
        }
    };
    match &def.value {
        VariableValue::Scalar(value) => add(value),
        VariableValue::Themed(entries) => entries.iter().for_each(|entry| add(&entry.value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_only_transitive_design_dependencies_and_retains_authored_aliases() {
        let doc: PenDocument = serde_json::from_value(serde_json::json!({
            "version":"1","variables":{
                "ink":{"type":"color","value":"$palette"},
                "palette":{"type":"color","value":"#abcdef"},
                "unrelated":{"type":"string","value":"Other project"}
            },
            "pages":[
                {"id":"canvas","name":"Canvas","children":[{"type":"ref","id":"button","ref":"master"}]},
                {"id":"lib","name":"Library","children":[
                    {"type":"frame","id":"master","children":[{"type":"ref","id":"icon","ref":"glyph"}]},
                    {"type":"rectangle","id":"glyph","fill":[{"type":"solid","color":"$ink"}]},
                    {"type":"text","id":"private","content":"An unrelated note"}
                ]}
            ]
        })).unwrap();
        let selection = HtmlSelection::new(
            &doc,
            &doc.pages.as_ref().unwrap()[0].children,
            &Default::default(),
        );
        let source = serde_json::to_string(&selection.source).unwrap();
        assert!(source.contains("$palette") && source.contains("$ink"));
        assert!(!source.contains("unrelated") && !source.contains("private"));
        let prepared = serde_json::to_string(&selection.prepared_document().children).unwrap();
        assert!(prepared.contains("button__icon"));
        assert!(prepared.contains("#abcdef"));
        assert!(!prepared.contains("$ink"));
    }

    #[test]
    fn independently_selected_master_child_and_instance_have_distinct_import_ids() {
        let doc: PenDocument = serde_json::from_value(serde_json::json!({
            "version":"1","children":[
                {"type":"frame","id":"master","children":[{"type":"text","id":"title","content":"Title"}]},
                {"type":"ref","id":"instance","ref":"master","descendants":{"title":{"content":"Override"}}}
            ]
        })).unwrap();
        let title = &doc.children[0].children().unwrap()[0];
        let selection = HtmlSelection::new(&doc, [&doc.children[1], title], &Default::default());
        let source: Value =
            serde_json::from_str(&selection.editable_source_json().unwrap()).unwrap();
        assert_eq!(
            source["pages"][0]["children"][0]["children"][0]["id"],
            "title"
        );
        assert_eq!(source["children"][0]["id"], "html-copy-title");
        assert_eq!(
            source["children"][1]["descendants"]["title"]["content"],
            "Override"
        );
    }
}
