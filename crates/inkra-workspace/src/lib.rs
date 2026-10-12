//! # inkra-workspace
//!
//! **Layer L4** (see SPEC 2.2).
//!
//! File system: platform paths, atomic writes, log files, and (later) the file tree
//! model, watching, search, recent files, session, autosave and recovery.

pub mod fs;
pub mod logfile;
pub mod paths;
pub mod time;

pub use paths::Paths;
