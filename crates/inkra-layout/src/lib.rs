//! # inkra-layout
//!
//! **Layer L3a** (see SPEC 2.2).
//!
//! Document layout: turns the document into a virtualized list of items (source
//! lines in source mode, blocks in live mode), each laid out into text runs with
//! exact source mappings plus decorations, with caches keyed by content, width,
//! state and style (AD-4). Provides hit testing, caret geometry and visual-row
//! navigation for the editor.

pub mod block;
pub mod fonts;
pub mod hash;
pub mod live;
pub mod run;
pub mod source;
pub mod style;
pub mod view;

pub use block::{BlockLayout, Deco, Hit, HitAction, Overlay};
pub use run::{InlineObject, MathScene, ObjectKind, Run, RunSpec, Seg, SourceMap};
pub use style::Style;
pub use view::{Ctx, Item, Mode, ViewLayout};

pub use epaint;
