//! Browser text shaping uses the same authored font, line height and wrapping as the canvas.
use super::paint_color;
use crate::{fmt_num, html_escape};
use jian_ops_schema::node::{
    FontWeight, TextAlign, TextAlignVertical, TextContent, TextFontStyle, TextGrowth, TextNode,
};
use jian_ops_schema::style::FontStyleKind;
use std::fmt::Write;

fn font_family(style: &mut String, family: &str) {
    if let Some(family) = super::tokens::reference(family) {
        let _ = write!(style, ";font-family:{family},sans-serif");
        return;
    }
    let mut quoted = String::new();
    for ch in family.chars() {
        match ch {
            '\'' | '\\' => {
                quoted.push('\\');
                quoted.push(ch);
            }
            ch if ch.is_control() => quoted.push(' '),
            ch => quoted.push(ch),
        }
    }
    let _ = write!(style, ";font-family:'{quoted}',sans-serif");
}

fn font_weight(weight: &FontWeight) -> u32 {
    match weight {
        FontWeight::Number(n) => *n,
        FontWeight::Keyword(s) => s.parse::<u32>().unwrap_or_else(|_| match s.as_str() {
            "thin" | "hairline" => 100,
            "extralight" | "extra-light" | "ultralight" | "ultra-light" => 200,
            "light" => 300,
            "medium" => 500,
            "semibold" | "semi-bold" | "demibold" => 600,
            "bold" => 700,
            "extrabold" | "extra-bold" | "ultrabold" | "ultra-bold" => 800,
            "black" | "heavy" => 900,
            _ => 400,
        }),
    }
}

fn decoration(style: &mut String, underline: Option<bool>, strike: Option<bool>) {
    match (underline, strike) {
        (Some(true), Some(true)) => style.push_str(";text-decoration:underline line-through"),
        (Some(true), _) => style.push_str(";text-decoration:underline"),
        (_, Some(true)) => style.push_str(";text-decoration:line-through"),
        _ => {}
    }
}

pub(super) fn style(style: &mut String, n: &TextNode) {
    // Block alignment keeps styled runs together on the same text line.
    style.push_str(";display:block");
    match n.text_align_vertical {
        Some(TextAlignVertical::Middle) => style.push_str(";align-content:center"),
        Some(TextAlignVertical::Bottom) => style.push_str(";align-content:end"),
        _ => {}
    }
    if matches!(
        n.text_growth,
        Some(TextGrowth::FixedWidth | TextGrowth::FixedWidthHeight)
    ) {
        style.push_str(";white-space:pre-wrap;overflow-wrap:break-word");
    } else {
        style.push_str(";white-space:pre");
    }
    if let Some(family) = &n.font_family {
        font_family(style, family);
    } else {
        style.push_str(";font-family:system-ui,sans-serif");
    }
    let _ = write!(
        style,
        ";font-size:{}px",
        fmt_num(n.font_size.unwrap_or(13.0))
    );
    if let Some(weight) = &n.font_weight {
        let token = match weight {
            FontWeight::Keyword(s) => super::tokens::reference(s),
            _ => None,
        };
        let value = token.unwrap_or_else(|| font_weight(weight).to_string());
        let _ = write!(style, ";font-weight:{value}");
    }
    if matches!(n.font_style, Some(TextFontStyle::Italic)) {
        style.push_str(";font-style:italic");
    }
    if let Some(n) = n.letter_spacing {
        let _ = write!(style, ";letter-spacing:{}px", fmt_num(n));
    }
    let _ = write!(
        style,
        ";line-height:{}",
        fmt_num(n.layout_line_height_multiplier().unwrap_or(1.2))
    );
    match n.text_align {
        Some(TextAlign::Center) => style.push_str(";text-align:center"),
        Some(TextAlign::Right) => style.push_str(";text-align:right"),
        Some(TextAlign::Justify) => style.push_str(";text-align:justify"),
        _ => style.push_str(";text-align:left"),
    }
    decoration(style, n.underline, n.strikethrough);
}

pub(super) fn body(out: &mut String, node: &TextNode) {
    match &node.content {
        TextContent::Plain(s) => out.push_str(&html_escape(s)),
        TextContent::Styled(segments) => {
            for seg in segments {
                let mut style = String::new();
                if let Some(family) = &seg.font_family {
                    font_family(&mut style, family);
                }
                if let Some(size) = seg.font_size {
                    let _ = write!(style, ";font-size:{size}px");
                }
                if let Some(weight) = seg.font_weight {
                    let _ = write!(style, ";font-weight:{weight}");
                }
                if let Some(font_style) = &seg.font_style {
                    style.push_str(if matches!(font_style, FontStyleKind::Italic) {
                        ";font-style:italic"
                    } else {
                        ";font-style:normal"
                    });
                }
                if let Some(color) = &seg.fill {
                    let _ = write!(style, ";color:{}", paint_color(color, 1.0));
                }
                decoration(&mut style, seg.underline, seg.strikethrough);
                let _ = write!(
                    out,
                    "<span style=\"{}\">{}</span>",
                    html_escape(style.trim_start_matches(';')),
                    html_escape(&seg.text)
                );
            }
        }
    }
}
