//! History-aware smart shuffle: avoid repeating recently played tracks.
//!
//! When enabled, advancing the queue picks a track outside the recent window
//! (last N indices) instead of plain sequential order. The window scales with
//! the playlist: `min(8, len - 1)`.

use std::collections::VecDeque;

/// How many recent indices to avoid (scaled to playlist length).
pub fn window_for_len(len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    8.min(len - 1)
}

/// Pick the next index in `[0, len)`, avoiding `recent` (most-recent last).
/// Falls back to plain sequential advance when everything is recent or the
/// window is empty. `seed` drives a tiny xorshift so picks vary per call.
pub fn pick_next(len: usize, current: usize, recent: &[usize], seed: u64) -> usize {
    if len <= 1 {
        return 0;
    }
    let window = window_for_len(len);
    if window == 0 {
        return (current + 1) % len;
    }
    let skip: std::collections::HashSet<usize> =
        recent.iter().rev().take(window).copied().collect();
    let candidates: Vec<usize> = (0..len).filter(|i| !skip.contains(i)).collect();
    if candidates.is_empty() {
        // Entire playlist is recent: fall back to sequential advance.
        return (current + 1) % len;
    }
    let mut state = seed ^ 0x9e3779b97f4a7c15;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    candidates[(state as usize) % candidates.len()]
}

/// Bounded recent-play window (most-recent last).
#[derive(Debug, Clone, Default)]
pub struct RecentWindow {
    items: VecDeque<usize>,
    cap: usize,
}

impl RecentWindow {
    pub fn new(len: usize) -> Self {
        Self {
            items: VecDeque::new(),
            cap: window_for_len(len).max(1),
        }
    }

    pub fn push(&mut self, idx: usize) {
        self.items.push_back(idx);
        while self.items.len() > self.cap {
            self.items.pop_front();
        }
    }

    pub fn as_slice(&self) -> Vec<usize> {
        self.items.iter().copied().collect()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avoids_recent_window() {
        // len 12 → window 8; indices 1..=8 are recent, so next must be 0, 9, 10 or 11.
        let recent: Vec<usize> = (1..=8).collect();
        for seed in 0..50 {
            let next = pick_next(12, 8, &recent, seed);
            assert!(
                [0, 9, 10, 11].contains(&next),
                "seed {seed} picked recent {next}"
            );
        }
    }

    #[test]
    fn small_playlist_sequential_fallback() {
        assert_eq!(pick_next(1, 0, &[], 0), 0);
        assert_eq!(pick_next(2, 0, &[1], 7), 0); // only non-recent is 0
        assert_eq!(pick_next(2, 1, &[0, 1], 3), 0); // all recent → wrap
    }

    #[test]
    fn window_scales_with_length() {
        assert_eq!(window_for_len(0), 0);
        assert_eq!(window_for_len(1), 0);
        assert_eq!(window_for_len(5), 4);
        assert_eq!(window_for_len(20), 8);
    }

    #[test]
    fn recent_window_caps() {
        let mut w = RecentWindow::new(20);
        for i in 0..20 {
            w.push(i);
        }
        assert_eq!(w.as_slice().len(), 8);
        assert_eq!(*w.as_slice().last().unwrap(), 19);
    }
}
