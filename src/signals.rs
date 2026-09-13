//! Ctrl-C / SIGTERM flag for the non-TTY player, which has no key loop to quit
//! from. The handler only stores into an atomic; the caller polls and unwinds
//! normally so MPV is stopped and statistics are saved.

use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Exit status conventionally reported for "terminated by SIGINT".
pub const SIGINT_EXIT_CODE: i32 = 130;

/// Whether a termination signal arrived since [`install`].
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

#[cfg(unix)]
pub fn install() {
    // A second signal means the caller is not unwinding fast enough, so give
    // the terminal back immediately instead of ignoring the user.
    extern "C" fn on_signal(_sig: libc::c_int) {
        if INTERRUPTED.swap(true, Ordering::Relaxed) {
            unsafe { libc::_exit(SIGINT_EXIT_CODE) };
        }
    }

    for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
    }
}

#[cfg(not(unix))]
pub fn install() {}

#[cfg(test)]
mod tests {
    use super::*;

    // One test: the flag is process-wide, so parallel tests would race on it.
    #[test]
    fn sigint_sets_the_flag_after_a_repeatable_install() {
        install();
        install();
        assert!(!interrupted());
        #[cfg(unix)]
        {
            unsafe { libc::raise(libc::SIGINT) };
            assert!(interrupted());
            INTERRUPTED.store(false, Ordering::Relaxed);
        }
    }
}
