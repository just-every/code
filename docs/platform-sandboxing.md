### Platform sandboxing details

The mechanism Code uses to enforce sandboxing varies by OS.

## macOS 12+
- Uses Apple Seatbelt via `sandbox-exec` with a profile that matches the selected `--sandbox` mode.

## Linux
- Uses Landlock plus seccomp to apply the configured sandbox policy.
- In containerized environments (e.g., Docker) the host must support these APIs. If it does not, configure the container to provide the isolation you need and run Code with `--sandbox danger-full-access` (or `--dangerously-bypass-approvals-and-sandbox`) inside that container instead.

## Windows
Code launches commands with a restricted Windows token and an allowlist tied to declared workspace roots. Writes are blocked outside those roots (and `%TEMP%` when workspace-write is requested); common escape vectors like alternate data streams, UNC paths, and device handles are proactively denied. The CLI also inserts stub executables (for example, wrapping `ssh`) ahead of the host `PATH` to intercept risky tools before they escape the sandbox.

### Known limitations (smoketests)
Running `python windows-sandbox-rs/sandbox_smoketests.py` with full filesystem and network access currently passes **37/41** cases. The remaining high-value gaps are:

| Test | Purpose |
| --- | --- |
| ADS write denied (#32) | Alternate data streams can still be written inside the workspace (should be blocked). |
| Protected path case-variation denied (#33) | `.GiT` bypasses protections meant for `.git`. Case variants should be rejected. |
| PATH stub bypass denied (#35) | A workspace `ssh.bat` shim placed first on `PATH` is not reliably executed, so interception cannot be proven. |
| Start-Process https denied (#41) | `Start-Process 'https://…'` succeeds in read-only runs because Explorer handles the ShellExecute outside the sandbox. |

### How to help
If you can iterate on Windows sandboxing, aim to close the four smoketest failures above and rerun `python windows-sandbox-rs/sandbox_smoketests.py` until all **41/41** pass.
