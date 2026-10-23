//! Which cat palette to draw: black on a light taskbar, white on a dark one.
//!
//! macOS needs none of this. Its tray icons are set as *template* images and
//! the menu bar re-tints them, so [`dark_taskbar`] is always `false` there and
//! the RGB of the frame is irrelevant.
//!
//! Windows has no template concept, so the app has to pick. The signal is
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` →
//! `SystemUsesLightTheme`, the same one RustCat reads — **not** `AppsUseLightTheme`.
//! The distinction matters: `AppsUseLightTheme` is what `window.theme()` reflects
//! (tao reads it for its dark-mode support), and Windows lets the two differ, so
//! "dark apps, light taskbar" is a normal configuration that would make the cat
//! invisible if we followed the window.
//!
//! The read is a hand-rolled `RegGetValueW` binding rather than the `windows` or
//! `windows-registry` crate: it is one call, and this way the build pulls no
//! Windows-only dependency for it.

/// `SystemUsesLightTheme` is a `DWORD`: 1 for a light taskbar, 0 for a dark one.
///
/// Only Windows calls this, but the tests exercise it everywhere, hence the
/// `cfg`: compiling it into a non-test build off Windows would make it dead code.
#[cfg(any(windows, test))]
fn taskbar_is_dark_from(value: u32) -> bool {
    value == 0
}

/// `true` when the Windows taskbar is dark, so the white frames should be used.
///
/// Always `false` off Windows (see the module docs). A failed registry read also
/// degrades to `false`: the light palette on a light taskbar is the harmless
/// guess, and the value is re-read on every sample anyway.
#[cfg(windows)]
pub fn dark_taskbar() -> bool {
    use core::ffi::c_void;

    /// `HKEY_CURRENT_USER`: the predefined handle is the pseudo-handle `0x80000001`.
    const HKEY_CURRENT_USER: *mut c_void = 0x8000_0001usize as *mut c_void;
    /// `RRF_RT_REG_DWORD` — only accept the value if it really is a `DWORD`.
    const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
    /// `ERROR_SUCCESS`.
    const ERROR_SUCCESS: i32 = 0;

    const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
    const VALUE: &str = "SystemUsesLightTheme";

    /// NUL-terminated UTF-16, the encoding the W entry points take.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(core::iter::once(0)).collect()
    }

    // Called once per sample, so the two allocations here are immaterial.
    let key = wide(KEY);
    let value_name = wide(VALUE);

    #[link(name = "advapi32")]
    unsafe extern "system" {
        /// `LSTATUS RegGetValueW(HKEY, LPCWSTR, LPCWSTR, DWORD, LPDWORD, PVOID, LPDWORD)`
        fn RegGetValueW(
            hkey: *mut c_void,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            value_type: *mut u32,
            data: *mut c_void,
            data_len: *mut u32,
        ) -> i32;
    }

    let mut data = 0u32;
    // Must be the buffer's size (4) on input; `RegGetValueW` fails with
    // `ERROR_MORE_DATA` if it is left at zero.
    let mut len = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_DWORD,
            core::ptr::null_mut(),
            (&raw mut data).cast::<c_void>(),
            &mut len,
        )
    };

    status == ERROR_SUCCESS && taskbar_is_dark_from(data)
}

#[cfg(not(windows))]
pub fn dark_taskbar() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_dark_and_one_is_light() {
        assert!(taskbar_is_dark_from(0));
        assert!(!taskbar_is_dark_from(1));
    }

    /// The registry read either succeeds or falls back; either way it must not
    /// panic. On this machine it also exercises the `not(windows)` path.
    #[test]
    fn reads_without_panicking() {
        let _ = dark_taskbar();
    }
}
