//! Translate the document's parent/child layout relationship into CSS.
use crate::{fmt_num, node_origin, node_size};
use jian_ops_schema::node::{
    AlignItems, ContainerProps, JustifyContent, LayoutMode, NumberOrExpression, Padding, PenNode,
};
use jian_ops_schema::sizing::{SizeLimits, SizingBehavior, SizingKeyword};
use op_editor_core::PenNodeExt;
use std::fmt::Write;

#[derive(Copy, Clone, PartialEq)]
pub(super) enum Parent {
    Canvas,
    Row,
    Column,
    Stack,
}

pub(super) fn container(node: &PenNode) -> Option<&ContainerProps> {
    match node {
        PenNode::Frame(n) => Some(&n.container),
        PenNode::Group(n) => Some(&n.container),
        PenNode::Rectangle(n) => Some(&n.container),
        _ => None,
    }
}

type Measures<'a> = (
    Option<&'a SizingBehavior>,
    Option<&'a SizingBehavior>,
    Option<&'a SizeLimits>,
);

fn measures(node: &PenNode) -> Measures<'_> {
    macro_rules! dims {
        ($n:expr) => {
            ($n.width.as_ref(), $n.height.as_ref(), Some(&$n.limits))
        };
    }
    match node {
        PenNode::Frame(n) => dims!(n.container),
        PenNode::Group(n) => dims!(n.container),
        PenNode::Rectangle(n) => dims!(n.container),
        PenNode::Text(n) => dims!(n),
        PenNode::Path(n) => dims!(n),
        PenNode::Ellipse(n) => dims!(n),
        PenNode::Image(n) => dims!(n),
        PenNode::Polygon(n) => dims!(n),
        PenNode::Tabs(n) => dims!(n),
        PenNode::TextInput(n) => dims!(n),
        PenNode::IconFont(n) => dims!(n),
        PenNode::TextArea(n) => dims!(n),
        PenNode::Select(n) => dims!(n),
        PenNode::Switch(n) => dims!(n),
        PenNode::Checkbox(n) => dims!(n),
        PenNode::Slider(n) => dims!(n),
        PenNode::RadioGroup(n) => dims!(n),
        PenNode::NumberInput(n) => dims!(n),
        PenNode::Progress(n) => dims!(n),
        PenNode::Line(_) | PenNode::Ref(_) => (None, None, None),
    }
}

impl super::Html {
    /// A standalone frame gets its own viewport. A docked flow panel with
    /// matching side gutters retains those insets in its responsive preview.
    pub(crate) fn standalone_frame_origin(
        node: &PenNode,
        parent: Option<&PenNode>,
    ) -> Option<(f64, f64)> {
        if !matches!(node, PenNode::Frame(_) | PenNode::Ref(_)) {
            return None;
        }
        let (x, y) = node_origin(node);
        if matches!(flow(node), Parent::Row | Parent::Column) {
            if let Some((parent_width, width)) =
                parent.and_then(PenNodeExt::width_px).zip(node.width_px())
            {
                if x > 0.0
                    && width > parent_width / 2.0
                    && (parent_width - width - 2.0 * x).abs() < 0.01
                {
                    return Some((x, y.clamp(0.0, x)));
                }
            }
        }
        Some((0.0, 0.0))
    }

    pub(crate) fn resolve_sizing(
        node: &mut PenNode,
        vars: Option<
            &std::collections::BTreeMap<String, jian_ops_schema::variable::VariableDefinition>,
        >,
        theme: &std::collections::BTreeMap<String, String>,
    ) {
        let resolve = |s: Option<&SizingBehavior>| match s {
            Some(SizingBehavior::Expression(value)) => {
                op_editor_core::variables_resolve::resolve_numeric_ref(value, vars, theme)
            }
            _ => None,
        };
        let (width, height, _) = measures(node);
        let (w, h) = (resolve(width), resolve(height));
        if let Some(w) = w {
            node.set_width_px(w);
        }
        if let Some(h) = h {
            node.set_height_px(h);
        }
    }
}

pub(super) fn flow(node: &PenNode) -> Parent {
    match container(node).and_then(|c| c.layout.as_ref()) {
        Some(LayoutMode::Horizontal) => Parent::Row,
        Some(LayoutMode::Vertical) => Parent::Column,
        Some(LayoutMode::None) => Parent::Stack,
        None => Parent::Canvas,
    }
}

pub(super) fn geometry(node: &PenNode, parent: Parent, standalone: bool) -> String {
    let mut style = String::from("box-sizing:border-box");
    let base = node.base();
    let positioned = base.x.is_some() || base.y.is_some();
    // Legacy flow children may retain an origin from a previous layout.
    // Match op-pen-loader's flow repair for overlays and constrained children.
    let absolute = match parent {
        Parent::Canvas => true,
        Parent::Stack => positioned,
        Parent::Row | Parent::Column => {
            base.role.as_deref() == Some("overlay") || (base.constraints.is_some() && positioned)
        }
    };
    let (x, y) = node_origin(node);
    // Taffy snaps both edges of a positioned vector. Fractional SVG
    // viewports otherwise accumulate visible drift across repeating paths.
    let snap_vector = absolute && matches!(node, PenNode::Path(_));
    if absolute {
        let (x, y) = if snap_vector {
            (x.round(), y.round())
        } else {
            (x, y)
        };
        let _ = write!(
            style,
            ";position:absolute;left:{}px;top:{}px",
            fmt_num(x),
            fmt_num(y)
        );
    } else {
        style.push_str(";position:relative");
        if parent == Parent::Stack {
            style.push_str(";grid-area:1 / 1");
        }
    }
    let (width, height, limits) = measures(node);
    let flex = !absolute && matches!(parent, Parent::Row | Parent::Column);
    for (axis, size, along) in [
        ("width", width, parent == Parent::Row),
        ("height", height, parent == Parent::Column),
    ] {
        match size {
            Some(SizingBehavior::Number(n)) => {
                let value = if snap_vector && *n > 0.0 {
                    let origin = if axis == "width" { x } else { y };
                    ((origin + n).round() - origin.round()).max(1.0)
                } else {
                    *n
                };
                let _ = write!(style, ";{axis}:{}px", fmt_num(value));
            }
            Some(SizingBehavior::Expression(s)) => {
                if let Some(length) = super::tokens::length(s) {
                    let _ = write!(style, ";{axis}:{length}");
                }
            }
            Some(SizingBehavior::Keyword(SizingKeyword::FitContent)) => {
                let _ = write!(style, ";{axis}:fit-content");
            }
            Some(SizingBehavior::Keyword(SizingKeyword::FillContainer)) if flex && along => {
                let min = if axis == "width"
                    && matches!(node, PenNode::Text(t) if !matches!(t.text_growth, Some(jian_ops_schema::node::TextGrowth::FixedWidth | jian_ops_schema::node::TextGrowth::FixedWidthHeight)))
                {
                    "min-content"
                } else {
                    "0"
                };
                let _ = write!(style, ";flex:1 1 0px;min-{axis}:{min}");
            }
            Some(SizingBehavior::Keyword(SizingKeyword::FillContainer)) if flex => {
                let _ = write!(style, ";align-self:stretch;{axis}:auto");
            }
            Some(SizingBehavior::Keyword(SizingKeyword::FillContainer)) => {
                let _ = write!(style, ";{axis}:100%");
            }
            _ => {}
        }
    }
    if flex
        && matches!(
            if parent == Parent::Row { width } else { height },
            Some(SizingBehavior::Number(_) | SizingBehavior::Expression(_))
        )
    {
        style.push_str(";flex-shrink:0");
    }
    if matches!(node, PenNode::Line(_)) {
        let (w, h) = node_size(node);
        let _ = write!(style, ";width:{}px;height:{}px", fmt_num(w), fmt_num(h));
    }
    // A selected frame keeps its canvas origin in a standalone page. Only
    // the space to the right of that origin is available on narrow screens.
    let authored_max = limits.and_then(|l| l.max_width).filter(|n| *n >= 0.0);
    if standalone
        && matches!(flow(node), Parent::Row | Parent::Column)
        && matches!(
            width,
            Some(SizingBehavior::Number(_) | SizingBehavior::Expression(_))
        )
    {
        let inset = base.x.unwrap_or(0.0);
        let available = if inset > 0.0 {
            format!("calc(100% - {}px)", fmt_num(inset))
        } else {
            "100%".into()
        };
        match authored_max {
            Some(max) => {
                let _ = write!(style, ";max-width:min({}px,{available})", fmt_num(max));
            }
            None => {
                let _ = write!(style, ";max-width:{available}");
            }
        }
    } else if let Some(max) = authored_max {
        let _ = write!(style, ";max-width:{}px", fmt_num(max));
    }
    if let Some(limits) = limits {
        for (prop, value) in [
            ("min-width", limits.min_width),
            ("min-height", limits.min_height),
            ("max-height", limits.max_height),
        ] {
            if let Some(n) = value.filter(|n| *n >= 0.0) {
                let _ = write!(style, ";{prop}:{}px", fmt_num(n));
            }
        }
    }
    style
}

pub(super) fn container_style(style: &mut String, node: &PenNode) {
    let Some(c) = container(node) else { return };
    if c.clip_content == Some(true) {
        style.push_str(";overflow:hidden");
    }
    let mode = flow(node);
    if mode == Parent::Canvas {
        return;
    }
    let align = match c.align_items {
        // Taffy clamps negative cross-axis free space to the start edge.
        Some(AlignItems::Center) if mode != Parent::Stack => "safe center",
        Some(AlignItems::End) if mode != Parent::Stack => "safe end",
        Some(AlignItems::Center) => "center",
        Some(AlignItems::End) => "end",
        Some(AlignItems::Stretch) => "stretch",
        _ => "start",
    };
    let justify = match c.justify_content {
        Some(JustifyContent::Center) => "center",
        Some(JustifyContent::End) => "end",
        Some(JustifyContent::SpaceBetween) if mode != Parent::Stack => "space-between",
        Some(JustifyContent::SpaceAround) if mode != Parent::Stack => "space-around",
        _ => "start",
    };
    if mode == Parent::Stack {
        let _ = write!(
            style,
            ";display:grid;justify-items:{justify};align-items:{align}"
        );
    } else {
        let dir = if mode == Parent::Row { "row" } else { "column" };
        let _ = write!(
            style,
            ";display:flex;flex-direction:{dir};justify-content:{justify};align-items:{align}"
        );
        match &c.gap {
            Some(NumberOrExpression::Number(n)) => {
                let _ = write!(style, ";gap:{}px", fmt_num(*n));
            }
            Some(NumberOrExpression::Expression(s)) => {
                if let Some(gap) = super::tokens::length(s) {
                    let _ = write!(style, ";gap:{gap}");
                }
            }
            None => {}
        }
    }
    let values: &[f64] = match &c.padding {
        Some(Padding::Uniform(n)) => std::slice::from_ref(n),
        Some(Padding::XY(v)) => v,
        Some(Padding::LtrB(v)) => v,
        Some(Padding::Expression(s)) => {
            if let Some(pad) = super::tokens::length(s) {
                let _ = write!(style, ";padding:{pad}");
            }
            return;
        }
        None => &[],
    };
    if !values.is_empty() {
        style.push_str(";padding:");
    }
    for (i, n) in values.iter().enumerate() {
        if i > 0 {
            style.push(' ');
        }
        let _ = write!(style, "{}px", fmt_num(*n));
    }
    // The native layout repair grows an open row to fit a taller child after
    // aligning children within its original content box. Reserve that growth
    // below the box so smaller siblings stay at their authored alignment.
    if mode == Parent::Row && c.clip_content != Some(true) {
        if let Some(height) = node.height_px() {
            let (top, bottom) = match values {
                [top, _, bottom, _] => (*top, *bottom),
                [vertical, ..] => (*vertical, *vertical),
                [] => (0.0, 0.0),
            };
            let tallest = crate::node_children(node)
                .iter()
                .filter(|child| {
                    let base = child.base();
                    base.role.as_deref() != Some("overlay")
                        && !(base.constraints.is_some() && (base.x.is_some() || base.y.is_some()))
                        && !crate::node_hidden(child)
                })
                .filter_map(PenNodeExt::height_px)
                .fold(0.0, f64::max);
            let growth = tallest + top + bottom - height;
            if tallest > 0.0 && growth > 0.5 {
                let _ = write!(
                    style,
                    ";height:{}px;padding-bottom:{}px",
                    fmt_num(height + growth),
                    fmt_num(bottom + growth)
                );
            }
        }
    }
}
