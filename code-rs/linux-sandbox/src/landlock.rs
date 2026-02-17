use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use code_core::error::CodexErr;
use code_core::error::Result;
use code_core::error::SandboxErr;
use code_core::protocol::SandboxPolicy;

use landlock::ABI;
use landlock::Access;
use landlock::AccessFs;
use landlock::CompatLevel;
use landlock::Compatible;
use landlock::Ruleset;
use landlock::RulesetAttr;
use landlock::RulesetCreatedAttr;
use seccompiler::BpfProgram;
use seccompiler::SeccompAction;
use seccompiler::SeccompCmpArgLen;
use seccompiler::SeccompCmpOp;
use seccompiler::SeccompCondition;
use seccompiler::SeccompFilter;
use seccompiler::SeccompRule;
use seccompiler::TargetArch;
use seccompiler::apply_filter;

/// Apply sandbox policies inside this thread so only the child inherits
/// them, not the entire CLI process.
pub(crate) fn apply_sandbox_policy_to_current_thread(
    sandbox_policy: &SandboxPolicy,
    cwd: &Path,
) -> Result<()> {
    if !sandbox_policy.has_full_network_access() {
        install_network_seccomp_filter_on_current_thread()?;
    }

    if !sandbox_policy.has_full_disk_write_access() {
        let writable_roots = sandbox_policy
            .get_writable_roots_with_cwd(cwd)
            .into_iter()
            .map(|writable_root| writable_root.root)
            .collect();
        install_filesystem_landlock_rules_on_current_thread(writable_roots)?;
    }

    // TODO(ragona): Add appropriate restrictions if
    // `sandbox_policy.has_full_disk_read_access()` is `false`.

    Ok(())
}

/// Installs Landlock file-system rules on the current thread allowing read
/// access to the entire file-system while restricting write access to
/// `/dev/null` and the provided list of `writable_roots`.
///
/// # Errors
/// Returns [`CodexErr::Sandbox`] variants when the ruleset fails to apply.
fn install_filesystem_landlock_rules_on_current_thread(writable_roots: Vec<PathBuf>) -> Result<()> {
    let abi = ABI::V5;
    let access_rw = AccessFs::from_all(abi);
    let access_ro = AccessFs::from_read(abi);
    let gpu_device_paths = gpu_device_paths(Path::new("/dev"));

    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(access_rw)?
        .create()?
        .add_rules(landlock::path_beneath_rules(&["/"], access_ro))?
        .add_rules(landlock::path_beneath_rules(&["/dev/null"], access_rw))?
        .set_no_new_privs(true);

    if !gpu_device_paths.is_empty() {
        ruleset = ruleset.add_rules(landlock::path_beneath_rules(&gpu_device_paths, access_rw))?;
    }

    if !writable_roots.is_empty() {
        ruleset = ruleset.add_rules(landlock::path_beneath_rules(&writable_roots, access_rw))?;
    }

    let status = ruleset.restrict_self()?;

    if status.ruleset == landlock::RulesetStatus::NotEnforced {
        return Err(CodexErr::Sandbox(SandboxErr::LandlockRestrict));
    }

    Ok(())
}

fn gpu_device_paths(dev_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let drm_path = dev_root.join("dri");
    if drm_path.exists() {
        paths.push(drm_path);
    }

    let amd_kfd_path = dev_root.join("kfd");
    if amd_kfd_path.exists() {
        paths.push(amd_kfd_path);
    }

    if let Ok(entries) = fs::read_dir(dev_root) {
        paths.extend(entries.flatten().filter_map(|entry| {
            let file_name = entry.file_name();
            let name = file_name.to_str()?;
            if name.starts_with("nvidia") {
                Some(entry.path())
            } else {
                None
            }
        }));
    }

    paths.sort();
    paths.dedup();
    paths
}

/// Installs a seccomp filter that blocks outbound network access except for
/// AF_UNIX domain sockets.
fn install_network_seccomp_filter_on_current_thread() -> std::result::Result<(), SandboxErr> {
    // Build rule map.
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();

    // Helper – insert unconditional deny rule for syscall number.
    let mut deny_syscall = |nr: i64| {
        rules.insert(nr, vec![]); // empty rule vec = unconditional match
    };

    deny_syscall(libc::SYS_connect);
    deny_syscall(libc::SYS_accept);
    deny_syscall(libc::SYS_accept4);
    deny_syscall(libc::SYS_bind);
    deny_syscall(libc::SYS_listen);
    deny_syscall(libc::SYS_getpeername);
    deny_syscall(libc::SYS_getsockname);
    deny_syscall(libc::SYS_shutdown);
    deny_syscall(libc::SYS_sendto);
    deny_syscall(libc::SYS_sendmsg);
    deny_syscall(libc::SYS_sendmmsg);
    // NOTE: allowing recvfrom allows some tools like: `cargo clippy` to run
    // with their socketpair + child processes for sub-proc management
    // deny_syscall(libc::SYS_recvfrom);
    deny_syscall(libc::SYS_recvmsg);
    deny_syscall(libc::SYS_recvmmsg);
    deny_syscall(libc::SYS_getsockopt);
    deny_syscall(libc::SYS_setsockopt);
    deny_syscall(libc::SYS_ptrace);

    // For `socket` we allow AF_UNIX (arg0 == AF_UNIX) and deny everything else.
    let unix_only_rule = SeccompRule::new(vec![SeccompCondition::new(
        0, // first argument (domain)
        SeccompCmpArgLen::Dword,
        SeccompCmpOp::Ne,
        libc::AF_UNIX as u64,
    )?])?;

    rules.insert(libc::SYS_socket, vec![unix_only_rule.clone()]);
    rules.insert(libc::SYS_socketpair, vec![unix_only_rule]); // always deny (Unix can use socketpair but fine, keep open?)

    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,                     // default – allow
        SeccompAction::Errno(libc::EPERM as u32), // when rule matches – return EPERM
        if cfg!(target_arch = "x86_64") {
            TargetArch::x86_64
        } else if cfg!(target_arch = "aarch64") {
            TargetArch::aarch64
        } else {
            unimplemented!("unsupported architecture for seccomp filter");
        },
    )?;

    let prog: BpfProgram = filter.try_into()?;

    apply_filter(&prog)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::gpu_device_paths;
    use std::collections::BTreeSet;
    use std::fs;

    #[test]
    fn gpu_device_paths_includes_expected_entries() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let dev_root = tempdir.path();

        fs::create_dir(dev_root.join("dri")).expect("dri directory should be created");
        fs::write(dev_root.join("kfd"), b"").expect("kfd should be created");
        fs::write(dev_root.join("nvidia0"), b"").expect("nvidia0 should be created");
        fs::write(dev_root.join("nvidiactl"), b"").expect("nvidiactl should be created");
        fs::write(dev_root.join("random"), b"").expect("random file should be created");

        let actual = gpu_device_paths(dev_root)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let expected = [
            dev_root.join("dri"),
            dev_root.join("kfd"),
            dev_root.join("nvidia0"),
            dev_root.join("nvidiactl"),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();

        assert_eq!(actual, expected);
    }

    #[test]
    fn gpu_device_paths_is_empty_when_no_gpu_entries_exist() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let dev_root = tempdir.path();

        fs::write(dev_root.join("null"), b"").expect("null should be created");
        fs::write(dev_root.join("random"), b"").expect("random should be created");

        assert!(gpu_device_paths(dev_root).is_empty());
    }
}
