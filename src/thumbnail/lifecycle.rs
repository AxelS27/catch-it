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
    paused: bool,
    pinned: bool,
    auto_close: bool,
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
            paused: false,
            pinned: false,
            auto_close: true,
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
                if !self.hovered
                    && !self.dragging
                    && !self.paused
                    && !self.pinned
                    && self.auto_close
                {
                    self.remaining = self
                        .remaining
                        .saturating_sub(now.saturating_duration_since(self.checkpoint));
                }
                self.checkpoint = now;
                self.hovered = hovered;
                if self.remaining.is_zero() && !self.paused && !self.pinned && self.auto_close {
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

    pub fn set_paused(&mut self, now: Instant, paused: bool) -> Action {
        let action = self.update(now, self.hovered);
        self.paused = paused;
        action
    }

    pub fn set_pinned(&mut self, now: Instant, pinned: bool) -> Action {
        if !self.can_drag() {
            return Action::None;
        }
        let action = self.update(now, self.hovered);
        self.pinned = pinned;
        if !pinned {
            self.remaining = self.timing.lifetime;
        }
        action
    }

    pub fn set_timeout(&mut self, now: Instant, lifetime: Option<Duration>) {
        if !self.can_drag() {
            return;
        }
        self.checkpoint = now;
        self.auto_close = lifetime.is_some();
        if let Some(lifetime) = lifetime {
            self.timing.lifetime = lifetime;
            self.remaining = lifetime;
        }
    }

    pub fn restart_entrance(&mut self, now: Instant) {
        if self.can_drag() {
            self.phase = Phase::Appearing;
            self.checkpoint = now;
        }
    }

    pub fn closed(&self) -> bool {
        self.phase == Phase::Closed
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
            Phase::Appearing if !self.paused => Some(self.timing.appear.saturating_sub(elapsed)),
            Phase::Visible
                if !self.hovered
                    && !self.dragging
                    && !self.paused
                    && !self.pinned
                    && self.auto_close =>
            {
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
    fn queued_and_capture_hidden_time_do_not_consume_remaining_budget() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, false);
        life.update(now + timing.appear, false);
        let pause = now + timing.appear + Duration::from_secs(2);
        life.set_paused(pause, true);
        assert_eq!(life.next_wake(pause), None);
        let resume = pause + Duration::from_secs(120);
        life.set_paused(resume, false);
        assert_eq!(life.next_wake(resume), Some(Duration::from_secs(3)));
        assert_eq!(
            life.update(resume + Duration::from_secs(3), false),
            Action::BeginDismiss
        );
    }

    #[test]
    fn pin_survives_timeout_and_unpin_starts_a_fresh_configured_interval() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, false);
        life.update(now + timing.appear, false);
        let pin = now + timing.appear + Duration::from_secs(2);
        life.set_pinned(pin, true);
        assert_eq!(
            life.update(pin + Duration::from_secs(3600), false),
            Action::None
        );
        assert_eq!(life.next_wake(pin + Duration::from_secs(3600)), None);
        let unpin = pin + Duration::from_secs(3600);
        life.set_pinned(unpin, false);
        assert_eq!(life.next_wake(unpin), Some(timing.lifetime));
        assert_eq!(
            life.update(unpin + timing.lifetime, false),
            Action::BeginDismiss
        );
    }

    #[test]
    fn never_and_changed_settings_do_not_prevent_explicit_close_or_revive_dismissal() {
        let now = Instant::now();
        let timing = Timing::default();
        let mut life = Lifecycle::new(now, timing, false);
        life.set_timeout(now, None);
        assert_eq!(
            life.update(now + Duration::from_secs(600), false),
            Action::None
        );
        assert_eq!(life.next_wake(now + Duration::from_secs(600)), None);
        let close = now + Duration::from_secs(601);
        life.dismiss(close);
        life.set_timeout(
            close + Duration::from_millis(100),
            Some(Duration::from_secs(15)),
        );
        assert_eq!(life.update(close + timing.dismiss, false), Action::Close);
    }

    #[test]
    fn changing_timer_while_pinned_applies_after_unpin() {
        let now = Instant::now();
        let mut life = Lifecycle::new(now, Timing::default(), false);
        life.set_pinned(now, true);
        life.update(now + Duration::from_secs(30), false);
        let change = now + Duration::from_secs(31);
        life.set_timeout(change, Some(Duration::from_secs(15)));
        assert_eq!(life.next_wake(change), None);
        life.set_pinned(change, false);
        assert_eq!(life.next_wake(change), Some(Duration::from_secs(15)));
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
