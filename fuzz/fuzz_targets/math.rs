//! Any TeX lays out or reports errors, never panics (2.4: unsupported commands
//! show the source in red).
#![no_main]

use inkra_math::{Request, check, render_uncached};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&flags, rest)) = data.split_first() else {
        return;
    };
    let Ok(tex) = std::str::from_utf8(rest) else {
        return;
    };
    let _ = check(tex);
    let _ = render_uncached(&Request::new(tex, flags & 1 == 1, 16.0));
});
