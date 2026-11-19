use std::{collections::BTreeMap, fs};

use op_html::{import_html, HtmlImportOptions};

use super::import_common::{
    import_result_to_outcome, parse_import_placement, parse_viewport_height,
};
use super::{McpTool, ToolErrorCode, ToolOutcome};

#[derive(Default)]
pub struct ImportHtml {
    variables: BTreeMap<String, jian_ops_schema::variable::VariableDefinition>,
    themes: BTreeMap<String, Vec<String>>,
}

impl McpTool for ImportHtml {
    fn name(&self) -> &str {
        "import_html"
    }

    fn call(&self, args: &BTreeMap<String, String>) -> ToolOutcome {
        let html = match args.get("html") {
            Some(html) => html.clone(),
            None => {
                let Some(path) = args.get("htmlPath").or_else(|| args.get("html_path")) else {
                    return ToolOutcome::Err(
                        ToolErrorCode::MissingArgument,
                        "html or htmlPath is required".into(),
                    );
                };
                match fs::read_to_string(path) {
                    Ok(html) => html,
                    Err(error) => {
                        return ToolOutcome::Err(
                            ToolErrorCode::ToolFailed,
                            format!("failed to read htmlPath {path:?}: {error}"),
                        );
                    }
                }
            }
        };
        if html.trim().is_empty() {
            return ToolOutcome::Err(
                ToolErrorCode::InvalidArgument,
                "html must not be empty".into(),
            );
        }
        let placement = match parse_import_placement(args) {
            Ok(placement) => placement,
            Err((code, message)) => return ToolOutcome::Err(code, message),
        };

        let viewport_height = match parse_viewport_height(args) {
            Ok(value) => value,
            Err((code, message)) => return ToolOutcome::Err(code, message),
        };
        let options = HtmlImportOptions {
            viewport_height,
            ..HtmlImportOptions::default()
        };
        let mut result = import_html(&html, &options);
        // Imported component libraries can share tokens with their source
        // document. Reuse equal definitions and protect unrelated designs.
        for (name, imported) in &result.variables {
            if self
                .variables
                .get(name)
                .is_some_and(|current| current != imported)
            {
                return ToolOutcome::Err(
                    ToolErrorCode::InvalidArgument,
                    format!("imported variable {name:?} conflicts with an existing definition"),
                );
            }
        }
        result
            .variables
            .retain(|name, _| !self.variables.contains_key(name));
        for (axis, values) in &mut result.themes {
            if let Some(existing) = self.themes.get(axis) {
                let mut merged = existing.clone();
                for value in values.iter() {
                    if !merged.contains(value) {
                        merged.push(value.clone());
                    }
                }
                *values = merged;
            }
        }
        result
            .themes
            .retain(|key, values| self.themes.get(key) != Some(values));
        import_result_to_outcome(result, placement)
    }
}

pub fn import_html_snapshot() -> ImportHtml {
    ImportHtml::default()
}

pub fn import_html_for_document(doc: &op_editor_core::EditorState) -> ImportHtml {
    ImportHtml {
        variables: doc.doc.variables.clone().unwrap_or_default(),
        themes: doc.doc.themes.clone().unwrap_or_default(),
    }
}
