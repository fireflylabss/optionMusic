//! Ctrl-C / SIGTERM flag for the non-TTY player, which has no key loop to quit
//! from. The handler only stores into an atomic; the caller polls and unwinds
//! normally so MPV is stopped and statistics are saved.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static SIGNAL: AtomicI32 = AtomicI32::new(SIGINT);

const SIGINT: i32 = 2;

/// Exit status conventionally reported for "terminated by SIGINT".
pub const SIGINT_EXIT_CODE: i32 = 128 + SIGINT;

/// Whether a termination signal arrived since [`install`].
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

/// `128 + signo`, the status a shell reports for the signal that arrived, so
/// SIGTERM and SIGHUP are distinguishable from Ctrl-C by the caller.
pub fn exit_code() -> i32 {
    128 + SIGNAL.load(Ordering::Relaxed)
}

#[cfg(unix)]
pub fn install() {
    // A second signal means the caller is not unwinding fast enough, so give
    // the terminal back immediately instead of ignoring the user.
    extern "C" fn on_signal(sig: libc::c_int) {
        SIGNAL.store(sig, Ordering::Relaxed);
        if INTERRUPTED.swap(true, Ordering::Relaxed) {
            unsafe { libc::_exit(128 + sig) };
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
            assert_eq!(exit_code(), SIGINT_EXIT_CODE);
            // A second signal without clearing the flag would `_exit`.
            INTERRUPTED.store(false, Ordering::Relaxed);
            unsafe { libc::raise(libc::SIGTERM) };
            assert!(interrupted());
            assert_eq!(exit_code(), 128 + libc::SIGTERM);
            INTERRUPTED.store(false, Ordering::Relaxed);
            SIGNAL.store(SIGINT, Ordering::Relaxed);
        }
    }
}
