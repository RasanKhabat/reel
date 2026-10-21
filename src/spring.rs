//! A damped harmonic oscillator pulled toward `target`.
//!
//! Tuned by response (roughly the settle period, seconds) and damping ratio (1 = no
//! overshoot). Integrated with small fixed sub-steps so behaviour doesn't depend on frame
//! rate. Changing `target` mid-flight keeps the current velocity, so interrupted
//! animations stay smooth.

#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
    pub response: f64,
    pub damping_ratio: f64,
}

pub const NORMAL_RESPONSE: f64 = 0.24;

impl Spring {
    pub fn new(v: f64) -> Self {
        Spring { value: v, velocity: 0.0, target: v, response: NORMAL_RESPONSE, damping_ratio: 1.0 }
    }

    pub fn is_settled(&self) -> bool {
        self.value == self.target && self.velocity == 0.0
    }

    pub fn snap(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
    }

    pub fn advance(&mut self, dt: f64) {
        if self.is_settled() {
            return;
        }
        let omega = 2.0 * std::f64::consts::PI / self.response;
        let stiffness = omega * omega;
        let damping = 2.0 * self.damping_ratio * omega;
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
        let mut s = Spring::new(0.0);
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
    fn retarget_keeps_velocity() {
        let mut s = Spring::new(0.0);
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
