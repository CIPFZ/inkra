//! # inkra-workspace
//!
//! **Layer L4** (see SPEC 2.2).
//!
//! File system: platform paths, atomic writes, log files, opening and saving
//! documents, crash recovery, session and recent files, file watching, and (M4)
//! the file tree model and search.

pub mod assets;
pub mod fileops;
pub mod files;
pub mod fs;
pub mod fuzzy;
pub mod index;
pub mod logfile;
pub mod net;
pub mod paths;
pub mod recovery;
pub mod search;
pub mod session;
pub mod time;
pub mod tree;
pub mod watch;

pub use paths::Paths;
