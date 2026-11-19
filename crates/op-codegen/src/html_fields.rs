//! Let the browser edit the very same box that paints the design field.
use super::{html_escape, paint_color};
use crate::{fmt_num, node_fill_css, parse_color};
use jian_ops_schema::node::{BoolOrExpression, CornerRadius, PenNode};
use op_editor_core::{fills, PenNodeExt};
use std::fmt::Write;

pub(super) fn emit(
    out: &mut String,
    node: &PenNode,
    attrs: &str,
    style: &str,
    indent: &str,
) -> bool {
    use super::super::{number_or_expr, opt_attr, opt_num_attr};

    let (element, radius, multiline, leading, trailing) = match node {
        PenNode::TextInput(n) => {
            let kind = if n.secure == Some(true) {
                "password"
            } else {
                "text"
            };
            (
                format!(
                    "<input type=\"{kind}\"{}{}",
                    opt_attr("placeholder", n.placeholder.as_deref()),
                    opt_attr("value", n.value.as_deref())
                ),
                &n.corner_radius,
                None,
                n.leading_icon.is_some(),
                n.trailing_icon.is_some(),
            )
        }
        PenNode::TextArea(n) => (
            format!(
                "<textarea{}",
                opt_attr("placeholder", n.placeholder.as_deref())
            ),
            &n.corner_radius,
            Some(n.value.as_deref().unwrap_or_default()),
            n.leading_icon.is_some(),
            n.trailing_icon.is_some(),
        ),
        PenNode::NumberInput(n) => (
            format!(
                "<input type=\"number\"{}{}{}{}",
                opt_num_attr("min", n.min),
                opt_num_attr("max", n.max),
                opt_num_attr("step", n.step),
                opt_num_attr("value", number_or_expr(&n.value))
            ),
            &n.corner_radius,
            None,
            false,
            false,
        ),
        _ => return false,
    };
    // Native canvas text fields use 8px horizontal padding, a 14px label
    // and a 6px intrinsic radius when no radius was authored.
    let px = |icon| if icon { 36 } else { 8 };
    let vertical = if multiline.is_some() { 8 } else { 0 };
    let mut css = format!("{style};appearance:none;margin:0;border:0;resize:none;padding:{vertical}px {}px {vertical}px {}px;font:400 14px/1 system-ui,sans-serif", px(trailing),px(leading));
    match radius {
        Some(CornerRadius::PerCorner(rs)) => {
            let _ = write!(
                css,
                ";border-radius:{}px {}px {}px {}px",
                rs[0], rs[1], rs[2], rs[3]
            );
        }
        Some(CornerRadius::Uniform(r)) => {
            let _ = write!(css, ";border-radius:{}px", fmt_num(*r));
        }
        None => css.push_str(";border-radius:6px"),
    }
    css.push_str(";color:#9ca3af");
    if let Some(fill) = node_fill_css(node) {
        if fills::first_solid_fill_opacity(node) >= 0.5 {
            if let Some((r, g, b, a)) = parse_color(&fill) {
                if a >= 0.5 {
                    let lin = |v: f32| {
                        if v <= 0.04045 {
                            v / 12.92
                        } else {
                            ((v + 0.055) / 1.055).powf(2.4)
                        }
                    };
                    let luminance = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
                    css.push_str(if luminance <= 0.17913 {
                        ";color:white"
                    } else {
                        ";color:black"
                    });
                }
            }
            // Keep foreground contrast live when a CSS colour token changes.
            let _ = write!(css, ";color:contrast-color({})", paint_color(&fill, 1.0));
        }
    } else {
        css.push_str(";background:transparent");
    }
    let disabled = if matches!(node.base().enabled, Some(BoolOrExpression::Bool(false))) {
        " disabled"
    } else {
        ""
    };
    let _ = write!(
        out,
        "{indent}{element}{attrs}{disabled} style=\"{}\"",
        html_escape(&css)
    );
    match multiline {
        Some(value) => {
            let _ = writeln!(
                out,
                ">{}{}</textarea>",
                if value.starts_with('\n') { "\n" } else { "" },
                html_escape(value)
            );
        }
        None => out.push_str(" />\n"),
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::{Codegen, Html};
    #[test]
    fn authored_field_is_the_sized_editable_element() {
        let doc=serde_json::from_str(r##"{"version":"1","children":[{"type":"text_input","id":"field","width":116,"height":32,"cornerRadius":6,"value":"28","fill":[{"type":"solid","color":"#25282d"}]}]}"##).unwrap();
        let html = Html.generate(&doc);
        let input = html
            .split("<input type=")
            .nth(1)
            .unwrap()
            .split(" />")
            .next()
            .unwrap();
        for expected in [
            "data-op-id=\"field\"",
            "width:116px",
            "height:32px",
            "border-radius:6px",
            "appearance:none",
            "color:white",
            "value=\"28\"",
        ] {
            assert!(input.contains(expected), "{expected}");
        }
    }
}
