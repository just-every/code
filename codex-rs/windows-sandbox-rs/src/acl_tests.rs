use super::acl_api_result;
use super::deny_ace_already_present;
use super::ensure_handle_is_not_filesystem_root;
use crate::token::LocalSid;
use pretty_assertions::assert_eq;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
use windows_sys::Win32::Storage::FileSystem::READ_CONTROL;

#[test]
fn deny_ace_update_failure_is_an_error() {
    let path = std::path::Path::new(r"C:\world-writable");
    let error = acl_api_result(path, "SetNamedSecurityInfoW", ERROR_ACCESS_DENIED)
        .expect_err("access denied must not look like an already-present ACE");

    assert_eq!(
        error.to_string(),
        r"SetNamedSecurityInfoW failed for C:\world-writable: 5"
    );
}

#[test]
fn deny_read_root_check_uses_the_open_handle() {
    let cwd = std::env::current_dir().expect("current directory");
    let root = cwd.ancestors().last().expect("filesystem root");
    let root_directory = OpenOptions::new()
        .access_mode(READ_CONTROL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(root)
        .expect("open filesystem root");
    let diagnostic_path = std::path::Path::new(r"C:\not-a-root");

    let error = ensure_handle_is_not_filesystem_root(&root_directory, diagnostic_path)
        .expect_err("classification must follow the root handle, not the diagnostic path");

    assert_eq!(
        error.to_string(),
        r"refusing to apply a deny-read ACE to filesystem root C:\not-a-root"
    );
}

#[test]
fn existing_deny_ace_is_visible_without_write_dac() {
    let target = tempfile::NamedTempFile::new().expect("temporary file");
    let sid = LocalSid::from_string("S-1-5-21-10-20-30-40").expect("test SID");
    let path = target.path();
    let psid = sid.as_ptr();
    assert!(unsafe { super::add_deny_read_ace(path, psid) }.expect("add deny ACE"));
    let already_present =
        unsafe { deny_ace_already_present(target.as_file(), path, psid, super::DenyAceKind::Read) }
            .expect("read existing deny ACE");
    assert!(already_present);
}
