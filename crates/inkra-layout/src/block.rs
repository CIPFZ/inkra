//! The layout of one item (a source line in source mode, a top-level block in
//! live mode).

use std::ops::Range;

use epaint::{Color32, Pos2, Rect, Stroke};

use crate::run::RunSpec;

/// A decoration painted behind or around text.
#[derive(Debug, Clone, PartialEq)]
pub enum Deco {
    /// A filled and/or stroked rectangle.
    Rect {
        /// Rectangle (block-local).
        rect: Rect,
        /// Fill.
        fill: Color32,
        /// Outline.
        stroke: Stroke,
        /// Corner radius.
        radius: f32,
    },
    /// A line segment.
    Line {
        /// Start (block-local).
        a: Pos2,
        /// End (block-local).
        b: Pos2,
        /// Stroke.
        stroke: Stroke,
    },
    /// A circle.
    Circle {
        /// Center (block-local).
        center: Pos2,
        /// Radius.
        radius: f32,
        /// Fill.
        fill: Color32,
        /// Outline.
        stroke: Stroke,
    },
}

/// Interactive region inside a block.
#[derive(Debug, Clone, PartialEq)]
pub enum HitAction {
    /// Toggle the task checkbox whose `[ ]` starts at this relative offset.
    ToggleTask(usize),
    /// Open a link.
    Link(String),
}

/// A clickable region.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Rectangle (block-local).
    pub rect: Rect,
    /// Action.
    pub action: HitAction,
}

/// Layout of one item.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockLayout {
    /// Total height including spacing.
    pub height: f32,
    /// Text runs.
    pub runs: Vec<RunSpec>,
    /// Decorations painted below text.
    pub decos: Vec<Deco>,
    /// Clickable regions.
    pub hits: Vec<Hit>,
    /// Relative source range the caret may occupy in this item (used to snap
    /// positions that fall on hidden markers).
    pub caret_range: Option<Range<usize>>,
}
