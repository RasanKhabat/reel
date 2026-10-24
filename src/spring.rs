//! A damped harmonic oscillator pulled toward `target`.
//!
//! Critically damped (no overshoot). Its stiffness is a natural frequency ω (rad/s): the
//! strip's scroll uses ω 13 and columns ω 15, the values the public web demo uses, scaled by
//! the user's animation speed. A spring can instead be given a fixed `response` (period in
//! seconds), as the swipe does while the fingers are down. Integrated with small fixed sub-steps so behaviour doesn't depend on frame
//! rate. Changing `target` mid-flight keeps the current velocity, so interrupted
//! animations stay smooth.

use std::sync::atomic::{AtomicU64, Ordering};

/// Natural frequency for scrolling the strip.
pub const SCROLL_OMEGA: f64 = 13.0;
/// Natural frequency for moving columns.
pub const COLUMN_OMEGA: f64 = 15.0;

/// User's animation speed multiplier (1 = the demo's feel), stored as f64 bits.
static SPEED: AtomicU64 = AtomicU64::new(0x3FF0_0000_0000_0000); // 1.0

pub fn speed() -> f64 {
    f64::from_bits(SPEED.load(Ordering::Relaxed))
}

/// Whether windows glide at all. Off: every move is instant (like macOS's Reduce Motion).
static ANIMATIONS_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn animations_on() -> bool {
    !ANIMATIONS_OFF.load(Ordering::Relaxed)
}

pub fn set_animations_on(on: bool) {
    ANIMATIONS_OFF.store(!on, Ordering::Relaxed);
}

pub fn set_speed(v: f64) {
    SPEED.store(v.clamp(0.25, 4.0).to_bits(), Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
    /// Natural frequency at speed 1×.
    pub omega: f64,
    /// Overrides `omega` with a fixed period (seconds) when set.
    pub response: Option<f64>,
    /// Experiment "eased curve": the current fixed-length ease, if one is running.
    tween: Option<Tween>,
}

/// A cubic Hermite ease from (`from`, `v0`) to (`to`, 0) over `duration` seconds. Starting
/// from the current velocity keeps retargeted moves smooth.
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f64,
    to: f64,
    v0: f64,
    t: f64,
    duration: f64,
}

/// Length of an eased move at speed 1×: about as long as the spring takes to look settled.
const EASE_DURATION: f64 = 0.42;

impl Spring {
    pub fn new(v: f64, omega: f64) -> Self {
        Spring { value: v, velocity: 0.0, target: v, omega, response: None, tween: None }
    }

    pub fn is_settled(&self) -> bool {
        self.value == self.target && self.velocity == 0.0
    }

    pub fn snap(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
        self.tween = None;
    }

    fn advance_eased(&mut self, dt: f64) {
        let fresh = !matches!(self.tween, Some(tw) if tw.to == self.target);
        if fresh {
            let duration = EASE_DURATION / speed() * if self.omega < COLUMN_OMEGA { 1.1 } else { 1.0 };
            self.tween = Some(Tween { from: self.value, to: self.target, v0: self.velocity, t: 0.0, duration });
        }
        let Some(mut tw) = self.tween else { return };
        tw.t += dt;
        let s = (tw.t / tw.duration).min(1.0);
        let (s2, s3) = (s * s, s * s * s);
        let (h00, h10, h01) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2);
        let (d00, d10, d01) = (6.0 * s2 - 6.0 * s, 3.0 * s2 - 4.0 * s + 1.0, -6.0 * s2 + 6.0 * s);
        self.value = h00 * tw.from + h10 * tw.duration * tw.v0 + h01 * tw.to;
        self.velocity = (d00 * tw.from + d10 * tw.duration * tw.v0 + d01 * tw.to) / tw.duration;
        self.tween = Some(tw);
        if s >= 1.0 {
            self.snap();
        }
    }

    pub fn advance(&mut self, dt: f64) {
        if self.is_settled() {
            return;
        }
        if self.response.is_none() && crate::experiments::on(crate::experiments::Exp::Eased) {
            self.advance_eased(dt);
            return;
        }
        self.tween = None;
        let omega = match self.response {
            Some(r) => 2.0 * std::f64::consts::PI / r,
            None => self.omega * speed(),
        };
        let stiffness = omega * omega;
        let damping = 2.0 * omega; // critically damped
        const SUBSTEP: f64 = 1.0 / 480.0;
        let mut remaining = dt;
        while remaining > 0.0 {
            let h = remaining.min(SUBSTEP);
            let accel = -stiffness * (self.value - self.target) - damping * self.velocity;
            self.velocity += accel * h; // semi-implicit Euler: velocity first,
            self.value += self.velocity * h; // then position with the new velocity
            remaining -= h;
        }
        if (self.value - self.target).abs() < 0.25 && self.velocity.abs() < 4.0 {
            self.snap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settles_without_overshoot() {
        let mut s = Spring::new(0.0, COLUMN_OMEGA);
        s.target = 500.0;
        let (mut peak, mut t) = (0.0f64, 0.0);
        while !s.is_settled() && t < 3.0 {
            s.advance(1.0 / 120.0);
            peak = peak.max(s.value);
            t += 1.0 / 120.0;
        }
        assert!(s.is_settled());
        assert_eq!(s.value, 500.0);
        assert!(t < 1.0);
        assert!(peak <= 500.5);
    }

    #[test]
    fn eased_reaches_target_smoothly() {
        let mut s = Spring::new(0.0, COLUMN_OMEGA);
        s.target = 500.0;
        let (mut prev, mut t) = (0.0f64, 0.0);
        while !s.is_settled() && t < 2.0 {
            s.advance_eased(1.0 / 120.0);
            assert!(s.value >= prev - 1e-9 && s.value <= 500.0 + 1e-9, "monotonic, no overshoot");
            prev = s.value;
            t += 1.0 / 120.0;
        }
        assert!(s.is_settled());
        assert_eq!(s.value, 500.0);
        assert!((t - EASE_DURATION).abs() < 0.02, "takes the set duration: {t}");
    }

    #[test]
    fn eased_retarget_keeps_velocity() {
        let mut s = Spring::new(0.0, COLUMN_OMEGA);
        s.target = 500.0;
        for _ in 0..10 {
            s.advance_eased(1.0 / 120.0);
        }
        let v = s.velocity;
        s.target = 900.0;
        s.advance_eased(1.0 / 1000.0);
        assert!((s.velocity - v).abs() / v < 0.1, "velocity carried over: {v} → {}", s.velocity);
    }

    #[test]
    fn retarget_keeps_velocity() {
        let mut s = Spring::new(0.0, COLUMN_OMEGA);
        s.target = 500.0;
        for _ in 0..10 {
            s.advance(1.0 / 120.0);
        }
        let v = s.velocity;
        s.target = 0.0;
        assert_eq!(s.velocity, v);
        s.advance(1.0 / 120.0);
        assert!(s.velocity < v);
    }
}
