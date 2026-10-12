//! Windows platform glue.
//!
//! This is the only module in the workspace that uses `unsafe`: two FFI calls into
//! Win32 that have no safe wrapper in an audited crate (SPEC AD-6 exception).

#![allow(unsafe_code)]

use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

/// Attaches to the console of the parent process (if any) so that `--version`
/// output of the GUI-subsystem executable is visible in a terminal.
pub fn attach_parent_console() {
    // SAFETY: AttachConsole has no memory-safety preconditions; failure (no parent
    // console) is reported through the return value, which we ignore.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Sets the AppUserModelID `io.inkra.Inkra` so the taskbar groups windows and
/// shows the right icon (UI-050).
pub fn set_app_user_model_id() {
    let id: Vec<u16> = "io.inkra.Inkra"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `id` is a valid NUL-terminated UTF-16 string that outlives the call;
    // the function copies it.
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
    }
}
