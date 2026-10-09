//! Hybrid push-to-talk, ported from the macOS DictationCoordinator:
//!
//! - hold the key, speak, release: dictation stops on release;
//! - quick tap (under 0.35 s): recording latches hands-free;
//! - next press while recording: stops it.

use std::time::{Duration, Instant};

pub const LATCH_THRESHOLD: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressAction {
    Start,
    Stop,
    Ignore,
}

#[derive(Debug, Default)]
pub struct PressTracker {
    pressed_at: Option<Instant>,
}

impl PressTracker {
    /// The push-to-talk key went down.
    pub fn press(&mut self, recording: bool, now: Instant) -> PressAction {
        if recording {
            self.pressed_at = None;
            PressAction::Stop
        } else {
            self.pressed_at = Some(now);
            PressAction::Start
        }
    }

    /// The push-to-talk key came up. A long hold stops; a quick tap latches.
    pub fn release(&mut self, now: Instant) -> PressAction {
        match self.pressed_at {
            Some(at) if now.duration_since(at) >= LATCH_THRESHOLD => {
                self.pressed_at = None;
                PressAction::Stop
            }
            _ => PressAction::Ignore,
        }
    }

    /// True while the press that started this recording has not ended in a
    /// latch, i.e. the user may still be holding the key for a shortcut.
    pub fn press_pending(&self) -> bool {
        self.pressed_at.is_some()
    }

    pub fn reset(&mut self) {
        self.pressed_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_then_release_stops() {
        let t0 = Instant::now();
        let mut p = PressTracker::default();
        assert_eq!(p.press(false, t0), PressAction::Start);
        assert_eq!(
            p.release(t0 + Duration::from_millis(900)),
            PressAction::Stop
        );
        assert!(!p.press_pending());
    }

    #[test]
    fn quick_tap_latches_and_next_press_stops() {
        let t0 = Instant::now();
        let mut p = PressTracker::default();
        assert_eq!(p.press(false, t0), PressAction::Start);
        assert_eq!(
            p.release(t0 + Duration::from_millis(120)),
            PressAction::Ignore
        );
        // Still recording hands-free; the next press stops.
        assert_eq!(
            p.press(true, t0 + Duration::from_secs(8)),
            PressAction::Stop
        );
        assert_eq!(
            p.release(t0 + Duration::from_millis(8_100)),
            PressAction::Ignore
        );
    }

    #[test]
    fn threshold_is_inclusive() {
        let t0 = Instant::now();
        let mut p = PressTracker::default();
        p.press(false, t0);
        assert_eq!(p.release(t0 + LATCH_THRESHOLD), PressAction::Stop);
    }
}
