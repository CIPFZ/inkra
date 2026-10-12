//! # inkra-workspace
//!
//! **Layer L4** (see SPEC 2.2).
//!
//! File system: platform paths, atomic writes, log files, opening and saving
//! documents, crash recovery, session and recent files, file watching, and (M4)
//! the file tree model and search.

pub mod files;
pub mod fs;
pub mod logfile;
pub mod paths;
pub mod recovery;
pub mod session;
pub mod time;
pub mod watch;

pub use paths::Paths;
