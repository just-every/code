//! Finds desktop-created directory ownership; storage is shared with sandbox setup.

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::path::PathBuf;
use std::ptr;

use anyhow::Context;
use anyhow::Result;
pub(crate) use codex_windows_sandbox::DesktopInstallation;
pub(crate) use codex_windows_sandbox::InstallationRecord;
pub(crate) use codex_windows_sandbox::load_sandbox_installation as load;
pub(crate) use codex_windows_sandbox::remove_sandbox_installation as remove;
pub(crate) use codex_windows_sandbox::save_sandbox_installation as save;
use codex_windows_sandbox::validate_local_directory_path;
use windows_sys::Win32::Foundation as foundation;
use windows_sys::Win32::UI::Shell::GetUserProfileDirectoryW;

const DESKTOP_INSTALLATION_MARKER: &str = ".desktop-created";

// Read this app-owned marker while impersonating the authenticated user. The
// desktop writes it only when creating the home. Cache cleanup does not need ownership.
pub(crate) fn read_desktop_installation(
    home: &Path,
    user_token: foundation::HANDLE,
) -> Result<DesktopInstallation> {
    let mut length = 0;
    unsafe { GetUserProfileDirectoryW(user_token, ptr::null_mut(), &mut length) };
    let mut profile = vec![0_u16; length as usize];
    if unsafe { GetUserProfileDirectoryW(user_token, profile.as_mut_ptr(), &mut length) } == 0 {
        return Err(io::Error::last_os_error()).context("read sandbox owner's profile directory");
    }
    let cache_home =
        PathBuf::from(OsString::from_wide(&profile[..length as usize - 1])).join(".cache");
    validate_local_directory_path(&cache_home)?;
    Ok(DesktopInstallation {
        created_codex_home: home.join(DESKTOP_INSTALLATION_MARKER).is_file(),
        cache_home,
    })
}
