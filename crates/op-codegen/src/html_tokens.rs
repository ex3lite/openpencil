//! The prepared document supplies the active value of each referenced token.
use crate::fmt_num;
use jian_ops_schema::variable::{VariableKind, VariableScalar, VariableValue};
use jian_ops_schema::PenDocument;
use std::fmt::Write;

// CSS escapes preserve punctuation in design token names. Replacing it with
// hyphens would merge e.g. "ink/subtle" with a separate "ink-subtle" token.
fn css_custom_property(name: &str) -> String {
    let mut css = String::from("--");
    for ch in name.strip_prefix("--").unwrap_or(name).chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            css.push(ch);
        } else {
            let _ = write!(css, "\\{:x} ", ch as u32);
        }
    }
    css
}

pub(super) fn reference(value: &str) -> Option<String> {
    value
        .strip_prefix('$')
        .filter(|name| !name.is_empty())
        .map(|name| format!("var({})", css_custom_property(name)))
}

pub(super) fn length(value: &str) -> Option<String> {
    reference(value).map(|r| format!("calc({r} * 1px)"))
}

// Authored colors are CSS color values, never CSS declarations or resources.
pub(super) fn safe_color(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " #%.,()+/-_".contains(c))
        && !value.to_ascii_lowercase().contains("url(")
}

pub(super) fn stylesheet(out: &mut String, doc: &PenDocument) {
    let Some(defs) = &doc.variables else { return };
    let mut rules = String::new();
    for (name, def) in defs {
        let VariableValue::Scalar(v) = &def.value else {
            continue;
        };
        let css = match v {
            VariableScalar::Num(n) => fmt_num(*n),
            VariableScalar::Bool(b) => b.to_string(),
            VariableScalar::Str(s) if s.starts_with('$') => reference(s).unwrap_or_default(),
            VariableScalar::Str(s) if matches!(def.kind, VariableKind::Color) && safe_color(s) => {
                s.clone()
            }
            VariableScalar::Str(s) if matches!(def.kind, VariableKind::String) => {
                let mut value = String::from("'");
                for c in s.chars() {
                    if c == '\'' || c == '\\' || c == '<' || c.is_control() {
                        let _ = write!(value, "\\{:x} ", c as u32);
                    } else {
                        value.push(c);
                    }
                }
                value.push('\'');
                value
            }
            _ => continue,
        };
        let _ = writeln!(rules, "  {}: {css};", css_custom_property(name));
    }
    if !rules.is_empty() {
        let _ = writeln!(out, "<style data-op-tokens>:root {{\n{rules}}}</style>");
    }
}
