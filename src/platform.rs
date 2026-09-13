//! Which platforms optionMusic actually claims to support.
//!
//! Everything here is Unix-shaped: libmpv is linked from the system, the TUI
//! assumes a POSIX terminal, and `cava`/`yt-dlp` are expected on `PATH`. Rather
//! than half-work on Windows, that target is rejected at compile time (see the
//! `compile_error!` in `lib.rs`), and untested Unixes say so out loud.

/// How much confidence the project has in the current build target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Built and exercised in CI.
    Tested,
    /// Expected to work, but nobody runs it: expect rough edges.
    BestEffort,
}

/// Support level of the platform this binary was built for.
pub const fn support() -> Support {
    if cfg!(target_os = "linux") {
        Support::Tested
    } else {
        Support::BestEffort
    }
}

/// `"linux · x86_64"`, for `msc version` and bug reports.
pub fn label() -> String {
    format!("{} · {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// One-line caveat to show on platforms nobody tests, `None` on Linux.
pub fn caveat() -> Option<String> {
    match support() {
        Support::Tested => None,
        Support::BestEffort => Some(format!(
            "{} is best-effort: only Linux is tested",
            std::env::consts::OS
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_names_os_and_arch() {
        let label = label();
        assert!(label.contains(std::env::consts::OS));
        assert!(label.contains(std::env::consts::ARCH));
    }

    #[test]
    fn linux_is_tested_and_quiet() {
        if cfg!(target_os = "linux") {
            assert_eq!(support(), Support::Tested);
            assert!(caveat().is_none());
        } else {
            assert_eq!(support(), Support::BestEffort);
            assert!(caveat().is_some_and(|c| c.contains("best-effort")));
        }
    }
}
