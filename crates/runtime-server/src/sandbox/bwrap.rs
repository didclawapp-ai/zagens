//! Bubblewrap (bwrap) sandbox backend for Linux (kernel-v2 Tier-1, M0.4).
//!
//! Adapted from upstream CodeWhale `crates/tui/src/sandbox/bwrap.rs` (MIT,
//! see `NOTICE.md`), rewritten against our [`SandboxPolicy`] model: writable
//! roots come from `policy.get_writable_roots` (cwd + configured roots +
//! tmp dirs) instead of cwd-only, read-only subpaths (`.zagens` /
//! `.deepseek` metadata) are re-protected inside writable roots, and network
//! isolation follows `policy.has_network_access()`.
//!
//! Bubblewrap is a setuid-less container runtime (used by Flatpak). It
//! creates a new mount namespace with configurable bind mounts, giving real
//! filesystem isolation without root. A typical invocation produced here:
//!
//! ```text
//! bwrap --ro-bind / / \
//!       --bind <root> <root> ...        # writable roots
//!       --ro-bind <root>/.zagens ...    # re-protected metadata
//!       --chdir <cwd> \
//!       --unshare-all [--share-net] \
//!       --die-with-parent \
//!       -- <program> <args>
//! ```
//!
//! Opt-in via root config key `prefer_bwrap = true`; when bwrap is not
//! installed, behavior is unchanged (Landlock declare-only fallback). We do
//! NOT vendor bwrap — install it via the distro package manager
//! (`apt/dnf/pacman install bubblewrap`).
//!
//! This is the Linux `enforced: true` prerequisite for letting dynamically
//! analyzed read-only shell commands enter parallel scheduling (proposal
//! §8.1.1): with a kernel-enforced read-only view, a misjudged "read-only"
//! command becomes a hard `EROFS` error instead of a silent write race.

#![cfg(target_os = "linux")]

use std::path::Path;

use super::policy::SandboxPolicy;

/// Canonical path to the bubblewrap binary.
pub const BWRAP_PATH: &str = "/usr/bin/bwrap";

/// Check if bubblewrap is installed and executable.
#[must_use]
pub fn is_available() -> bool {
    if Path::new(BWRAP_PATH).exists() {
        return true;
    }
    // Fall back to PATH lookup for distros that install outside /usr/bin.
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join("bwrap").is_file()))
}

/// Resolve the bwrap binary to invoke (canonical path preferred).
fn bwrap_program() -> String {
    if Path::new(BWRAP_PATH).exists() {
        BWRAP_PATH.to_string()
    } else {
        "bwrap".to_string()
    }
}

/// Build a bwrap command wrapping `program args` under `policy`.
///
/// The returned vector replaces `ExecEnv.command`. The root filesystem is
/// bind-mounted read-only; each policy writable root gets a read-write bind
/// with its read-only subpaths re-protected on top (later bwrap binds
/// override earlier ones). All namespaces are unshared; the network
/// namespace is shared back only when the policy allows network access.
#[must_use]
pub fn build_bwrap_command(
    policy: &SandboxPolicy,
    cwd: &Path,
    program: &str,
    args: &[String],
) -> Vec<String> {
    let mut cmd: Vec<String> = Vec::with_capacity(16 + args.len());

    cmd.push(bwrap_program());

    // Read-only view of the entire root filesystem.
    cmd.push("--ro-bind".to_string());
    cmd.push("/".to_string());
    cmd.push("/".to_string());

    // Writable roots from the policy (cwd, configured roots, tmp dirs).
    for writable in policy.get_writable_roots(cwd) {
        if !writable.root.exists() {
            continue;
        }
        let root = writable.root.to_string_lossy().to_string();
        cmd.push("--bind".to_string());
        cmd.push(root.clone());
        cmd.push(root);

        // Re-protect metadata subpaths inside the writable root.
        for subpath in &writable.read_only_subpaths {
            if !subpath.exists() {
                continue;
            }
            let subpath = subpath.to_string_lossy().to_string();
            cmd.push("--ro-bind".to_string());
            cmd.push(subpath.clone());
            cmd.push(subpath);
        }
    }

    // Run from the working directory inside the container.
    cmd.push("--chdir".to_string());
    cmd.push(cwd.to_string_lossy().to_string());

    // Maximum namespace isolation; selectively share the network back.
    cmd.push("--unshare-all".to_string());
    if policy.has_network_access() {
        cmd.push("--share-net".to_string());
    }

    // Kill the sandboxed process tree if the runtime dies.
    cmd.push("--die-with-parent".to_string());

    cmd.push("--".to_string());
    cmd.push(program.to_string());
    cmd.extend(args.iter().cloned());

    cmd
}

/// Detect whether a command failure was caused by bwrap isolation.
#[must_use]
pub fn detect_denial(exit_code: i32, stderr: &str) -> bool {
    if exit_code == 0 {
        return false;
    }
    let lower = stderr.to_ascii_lowercase();
    lower.contains("read-only file system")
        || lower.contains("bwrap:")
        || (lower.contains("permission denied") && lower.contains("bwrap"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn shell_args() -> Vec<String> {
        vec!["-c".to_string(), "echo hi".to_string()]
    }

    #[test]
    fn is_available_does_not_panic() {
        let _ = is_available();
    }

    #[test]
    fn read_only_policy_has_no_writable_binds() {
        let cmd = build_bwrap_command(
            &SandboxPolicy::ReadOnly,
            Path::new("/home/user/project"),
            "sh",
            &shell_args(),
        );
        assert!(cmd.iter().any(|a| a == "--ro-bind"));
        assert!(!cmd.iter().any(|a| a == "--bind"), "{cmd:?}");
        assert!(cmd.iter().any(|a| a == "--unshare-all"));
        assert!(!cmd.iter().any(|a| a == "--share-net"));
        assert_eq!(cmd[cmd.len() - 1], "echo hi");
        assert_eq!(cmd[cmd.len() - 3], "sh");
    }

    #[test]
    fn workspace_write_binds_existing_cwd_and_respects_network() {
        let cwd = std::env::temp_dir().join("zagens-bwrap-test-cwd");
        let _ = std::fs::create_dir_all(&cwd);
        let policy = SandboxPolicy::workspace_with_roots(vec![PathBuf::from("/nonexistent")], true);
        let cmd = build_bwrap_command(&policy, &cwd, "sh", &shell_args());

        assert!(cmd.iter().any(|a| a == "--bind"), "{cmd:?}");
        assert!(cmd.iter().any(|a| a == "--share-net"));
        // Non-existent configured roots are skipped, not bound.
        assert!(!cmd.iter().any(|a| a == "/nonexistent"), "{cmd:?}");
        // chdir points at the working directory.
        let chdir_idx = cmd.iter().position(|a| a == "--chdir").expect("--chdir");
        assert!(cmd[chdir_idx + 1].contains("zagens-bwrap-test-cwd"));
    }

    #[test]
    fn denial_detection_matches_erofs() {
        assert!(detect_denial(1, "sh: touch: Read-only file system"));
        assert!(detect_denial(1, "bwrap: cannot mount"));
        assert!(!detect_denial(0, "Read-only file system"));
        assert!(!detect_denial(1, "normal failure"));
    }
}
