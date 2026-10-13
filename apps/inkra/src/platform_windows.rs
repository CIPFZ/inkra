//! Windows platform glue.
//!
//! This is the only module in the workspace that uses `unsafe`: three FFI calls
//! into Win32 that have no safe wrapper in an audited crate (SPEC AD-6
//! exception).

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

/// Whether the user turned off animations ("Show animations in Windows"; UI-033).
pub fn reduce_motion() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut on: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to `pvParam`, which points
    // to a live, properly aligned i32 for the duration of the call.
    let ok =
        unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut on).cast(), 0) };
    ok != 0 && on == 0
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
