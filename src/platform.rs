//! Which platforms optionMusic actually claims to support.
//!
//! Linux is the one target that is developed against and packaged; everything
//! else builds (CI covers macOS and Windows) but nobody runs it day to day, so
//! those builds say so out loud instead of pretending to be equal.

/// How much confidence the project has in the current build target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// Developed against and packaged.
    Tested,
    /// Builds in CI, but nobody runs it: expect rough edges.
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
