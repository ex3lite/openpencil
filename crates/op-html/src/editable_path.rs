//! A selected vector exported by OpenBrush has a tight SVG viewport and one
//! ordinary path element. Read its current SVG/CSS values after an HTML edit.

use crate::color::parse_css_color;
use crate::css::cascade::{compute_style_for_viewport, ComputedStyle};
use crate::dom::{DomElement, DomNode};
use crate::mapper::MapCtx;
use jian_ops_schema::node::path::PathFillRule;
use jian_ops_schema::node::{PathNode, PenNode};
use jian_ops_schema::style::{PenStroke, StrokeAlign, StrokeCap, StrokeJoin, StrokeThickness};

pub(super) fn map(
    context: &mut MapCtx<'_>,
    ancestors: &[&DomElement],
    svg: &DomElement,
    style: &ComputedStyle,
) -> Option<PenNode> {
    svg.attr("data-op-id")?;
    if svg.attr("preserveaspectratio") != Some("none") {
        return None;
    }
    let mut elements = svg.children.iter().filter_map(|node| match node {
        DomNode::Element(e) => Some(e),
        DomNode::Text(_) => None,
    });
    let shape = elements.next()?;
    if shape.tag != "path" || elements.next().is_some() || shape.attr("transform").is_some() {
        return None;
    }
    let d = shape.attr("d")?.trim();
    let mut chain = ancestors.to_vec();
    chain.push(shape);
    let shape_style = compute_style_for_viewport(
        &chain,
        context.rules,
        Some(style),
        context.opts.base_font_size,
        context.opts.viewport_width,
        context.opts.viewport_height(),
    );
    let prop = |key: &str| {
        shape_style
            .get(key)
            .or_else(|| shape.attr(key))
            .or_else(|| style.get(key))
            .or_else(|| svg.attr(key))
    };
    if ["transform", "filter", "mask", "clip-path"]
        .iter()
        .any(|key| prop(key).is_some_and(|v| v != "none"))
    {
        return None;
    }
    let color = |v: &str| {
        let paint = if v.eq_ignore_ascii_case("currentColor") {
            shape_style.get("color").unwrap_or("black")
        } else {
            v
        };
        parse_css_color(paint).map(|c| vec![crate::solid_fill(&c)])
    };
    let fill = match prop("fill").unwrap_or("black") {
        "none" => None,
        v => Some(color(v)?),
    };
    let stroke = match prop("stroke").unwrap_or("none") {
        "none" => None,
        v => Some(PenStroke {
            thickness: StrokeThickness::Uniform(
                prop("stroke-width")
                    .unwrap_or("1")
                    .trim_end_matches("px")
                    .parse()
                    .ok()?,
            ),
            align: Some(StrokeAlign::Center),
            join: prop("stroke-linejoin").and_then(|v| match v {
                "round" => Some(StrokeJoin::Round),
                "bevel" => Some(StrokeJoin::Bevel),
                "miter" => Some(StrokeJoin::Miter),
                _ => None,
            }),
            cap: prop("stroke-linecap").and_then(|v| match v {
                "round" => Some(StrokeCap::Round),
                "square" => Some(StrokeCap::Square),
                "butt" => Some(StrokeCap::None),
                _ => None,
            }),
            dash_pattern: prop("stroke-dasharray")
                .filter(|v| *v != "none")
                .map(|v| {
                    v.split(|c: char| c == ',' || c.is_ascii_whitespace())
                        .filter(|part| !part.is_empty())
                        .map(str::parse)
                        .collect::<Result<Vec<f32>, _>>()
                })
                .transpose()
                .ok()?,
            dash_offset: prop("stroke-dashoffset").and_then(|v| v.parse().ok()),
            fill: Some(color(v)?),
        }),
    };
    let props = super::visual_props(context, style);
    let mut width = props.width.or_else(|| super::numeric_attr(svg, "width"));
    let mut height = props.height.or_else(|| super::numeric_attr(svg, "height"));
    let mut base = super::base_with_sizing(
        context,
        "path",
        style,
        &mut width,
        &mut height,
        props.limits,
    );
    base.name = svg
        .attr("data-op-name")
        .or_else(|| svg.attr("data-op-id"))
        .map(str::to_owned);
    let node = PenNode::Path(PathNode {
        base,
        d: Some(d.to_string()),
        width,
        height,
        fill,
        stroke,
        fill_rule: match prop("fill-rule") {
            Some("evenodd") => Some(PathFillRule::Evenodd),
            _ => None,
        },
        effects: props.effects,
        limits: props.limits,
        icon_id: None,
        anchors: None,
        closed: None,
        mask: None,
        state: None,
        bindings: None,
        events: None,
        lifecycle: None,
        semantics: None,
        gestures: None,
        route: None,
    });
    Some(super::finish(context, node))
}
