use std::time::{Duration, Instant};

/// Provisional interaction profile. Not a measured macOS specification.
#[derive(Clone, Copy)]
pub struct Timing {
    pub appear: Duration,
    pub lifetime: Duration,
    pub dismiss: Duration,
}

impl Timing {
    pub fn for_motion_enabled(enabled: bool) -> Self {
        if enabled {
            return Self::default();
        }
        Self {
            appear: Duration::from_millis(1),
            dismiss: Duration::from_millis(1),
            ..Self::default()
        }
    }
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            appear: Duration::from_millis(180),
            lifetime: Duration::from_secs(5),
            dismiss: Duration::from_millis(180),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    BeginDismiss,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Appearing,
    Visible,
    Dismissing,
    Closed,
}

/// Monotonic, event-driven timing: no frame-counting and no polling during hover.
pub struct Lifecycle {
    phase: Phase,
    timing: Timing,
    checkpoint: Instant,
    remaining: Duration,
    hovered: bool,
    dragging: bool,
}

impl Lifecycle {
    pub fn new(now: Instant, timing: Timing, hovered: bool) -> Self {
        Self {
            phase: Phase::Appearing,
            timing,
            checkpoint: now,
            remaining: timing.lifetime,
            hovered,
            dragging: false,
        }
    }

    pub fn update(&mut self, now: Instant, hovered: bool) -> Action {
        match self.phase {
            Phase::Appearing => {
                self.hovered = hovered;
                if now.saturating_duration_since(self.checkpoint) >= self.timing.appear {
                    self.phase = Phase::Visible;
                    self.checkpoint += self.timing.appear;
                    // Start the lifetime only after the entrance animation completes.
                    return self.update(now, hovered);
                }
            }
            Phase::Visible => {
                if !self.hovered && !self.dragging {
                    self.remaining = self
                        .remaining
                        .saturating_sub(now.saturating_duration_since(self.checkpoint));
                }
                self.checkpoint = now;
                self.hovered = hovered;
                if self.remaining.is_zero() {
                    return self.dismiss(now);
                }
            }
            Phase::Dismissing => {
                if now.saturating_duration_since(self.checkpoint) >= self.timing.dismiss {
                    self.phase = Phase::Closed;
                    return Action::Close;
                }
            }
            Phase::Closed => return Action::Close,
        }
        Action::None
    }

    pub fn can_drag(&self) -> bool {
        matches!(self.phase, Phase::Appearing | Phase::Visible)
    }

    pub fn begin_drag(&mut self, now: Instant, hovered: bool) -> Action {
        let action = self.update(now, hovered);
        if self.can_drag() {
            self.dragging = true;
        }
        action
    }

    pub fn end_drag(&mut self, now: Instant, hovered: bool) -> Action {
        // Account for the whole modal loop while the drag pause is still active.
        let action = self.update(now, hovered);
        self.dragging = false;
        action
    }

    pub fn dismiss(&mut self, now: Instant) -> Action {
        match self.phase {
            Phase::Appearing | Phase::Visible => {
                self.phase = Phase::Dismissing;
                self.checkpoint = now;
                Action::BeginDismiss
            }
            Phase::Dismissing => Action::None,
            Phase::Closed => Action::Close,
        }
    }

    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        let elapsed = now.saturating_duration_since(self.checkpoint);
        match self.phase {
            Phase::Appearing => Some(self.timing.appear.saturating_sub(elapsed)),
            Phase::Visible if !self.hovered && !self.dragging => {
                Some(self.remaining.saturating_sub(elapsed))
            }
            Phase::Dismissing => Some(self.timing.dismiss.saturating_sub(elapsed)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetime_begins_after_appearance_and_keeps_exit_alive() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, false);
        assert_eq!(life.update(now + timing.appear, false), Action::None);
        assert_eq!(life.next_wake(now + timing.appear), Some(timing.lifetime));
        let end = now + timing.appear + timing.lifetime;
        assert_eq!(life.update(end, false), Action::BeginDismiss);
        assert_eq!(life.update(end + timing.dismiss / 2, true), Action::None);
        assert_eq!(life.update(end + timing.dismiss, true), Action::Close);
    }

    #[test]
    fn hover_pauses_remaining_time_instead_of_resetting_it() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, false);
        life.update(now + timing.appear, false);
        let enter = now + timing.appear + Duration::from_secs(2);
        life.update(enter, true);
        assert_eq!(life.next_wake(enter), None);
        let leave = enter + Duration::from_secs(60);
        assert_eq!(life.update(leave, false), Action::None);
        assert_eq!(life.next_wake(leave), Some(Duration::from_secs(3)));
        assert_eq!(
            life.update(leave + Duration::from_secs(3), false),
            Action::BeginDismiss
        );
    }

    #[test]
    fn initial_hover_and_repeated_hover_do_not_consume_budget() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, true);
        life.update(now + timing.appear, true);
        life.update(now + Duration::from_secs(20), true);
        let leave = now + Duration::from_secs(30);
        life.update(leave, false);
        assert_eq!(life.next_wake(leave), Some(timing.lifetime));
    }

    #[test]
    fn explicit_dismiss_is_idempotent_and_hover_cannot_revive_it() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, true);
        assert_eq!(life.dismiss(now), Action::BeginDismiss);
        assert_eq!(life.dismiss(now + Duration::from_millis(20)), Action::None);
        assert_eq!(life.update(now + timing.dismiss, true), Action::Close);
    }

    #[test]
    fn active_drag_pauses_even_after_mouse_leaves_and_cancel_resumes_remaining_time() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, false);
        life.update(now + timing.appear, false);
        let start = now + timing.appear + Duration::from_secs(2);
        assert_eq!(life.begin_drag(start, true), Action::None);
        assert_eq!(
            life.update(start + Duration::from_secs(30), false),
            Action::None
        );
        assert_eq!(life.next_wake(start + Duration::from_secs(30)), None);
        let end = start + Duration::from_secs(60);
        assert_eq!(life.end_drag(end, false), Action::None);
        assert_eq!(life.next_wake(end), Some(Duration::from_secs(3)));
        assert_eq!(
            life.update(end + Duration::from_secs(3), false),
            Action::BeginDismiss
        );
        assert!(!life.can_drag());
    }

    #[test]
    fn drag_during_entrance_does_not_consume_the_lifetime() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, false);
        assert_eq!(
            life.begin_drag(now + Duration::from_millis(50), true),
            Action::None
        );
        let end = now + Duration::from_secs(60);
        assert_eq!(life.end_drag(end, false), Action::None);
        assert_eq!(life.next_wake(end), Some(timing.lifetime));
    }

    #[test]
    fn disabling_motion_does_not_change_idle_lifetime() {
        let timing = Timing::for_motion_enabled(false);
        assert_eq!(timing.lifetime, Timing::default().lifetime);
        assert_eq!(timing.appear, Duration::from_millis(1));
        assert_eq!(timing.dismiss, Duration::from_millis(1));
    }

    #[test]
    fn delayed_timer_accounts_for_elapsed_time() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, false);
        assert_eq!(
            life.update(now + Duration::from_secs(10), false),
            Action::BeginDismiss
        );
        assert_eq!(
            life.next_wake(now + Duration::from_secs(10)),
            Some(timing.dismiss)
        );
    }
}
