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
    /// An image (painted by the host, which owns textures).
    Image {
        /// Rectangle (block-local).
        rect: Rect,
        /// Image source as written in the document.
        src: String,
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

/// Host-drawn widgets over a block (e.g. code block tools, ED-002).
#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    /// A code block: its box (block-local), language and code range.
    Code {
        /// Box.
        rect: Rect,
        /// Language word of the info string.
        lang: String,
        /// Absolute range of the info string, if any.
        info: Option<Range<usize>>,
        /// Absolute range of the code content.
        body: Range<usize>,
    },
    /// A table: its grid (block-local) and source range.
    Table {
        /// Grid.
        rect: Rect,
        /// Absolute source range.
        span: Range<usize>,
        /// Column widths.
        cols: Vec<f32>,
    },
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
    /// Host-drawn overlays.
    pub overlays: Vec<Overlay>,
}
