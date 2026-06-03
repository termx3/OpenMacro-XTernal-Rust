// SPDX-License-Identifier: AGPL-3.0-only
//! PID-style reel controller. Pure math, so it is trivially unit-testable —
//! this is the payoff of keeping the engine free of platform dependencies.

use crate::reader::ReelContext;

/// Control gains. Defaults mirror the `main` settings block in `Constants.ahk`
/// (proportional_gain, derivative_gain, neutral_duty_cycle).
#[derive(Debug, Clone, Copy)]
pub struct Gains {
    pub proportional: f64,
    pub derivative: f64,
    pub neutral_duty_cycle: f64,
}

impl Default for Gains {
    fn default() -> Self {
        Self { proportional: 0.42, derivative: 0.55, neutral_duty_cycle: 0.5 }
    }
}

/// Decides the mouse-hold duty cycle (`0.0..=1.0`) from the reel bar error.
#[derive(Debug, Default)]
pub struct FishingController {
    gains: Gains,
    last_error: Option<f64>,
}

impl FishingController {
    pub fn new(gains: Gains) -> Self {
        Self { gains, last_error: None }
    }

    /// Clear derivative history (call when the reel context is lost).
    pub fn reset(&mut self) {
        self.last_error = None;
    }

    /// Proportional + derivative control around a neutral duty cycle.
    /// Returns the desired hold fraction, clamped to `0.0..=1.0`.
    pub fn update(&mut self, ctx: &ReelContext) -> f64 {
        let error = ctx.fish_position - ctx.bar_position;
        let derivative = match self.last_error {
            Some(prev) => error - prev,
            None => 0.0,
        };
        self.last_error = Some(error);

        let output = self.gains.neutral_duty_cycle
            + self.gains.proportional * error
            + self.gains.derivative * derivative;

        output.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(fish: f64, bar: f64) -> ReelContext {
        ReelContext { fish_position: fish, bar_position: bar, bar_width: 0.1 }
    }

    #[test]
    fn neutral_when_aligned() {
        let mut c =
            FishingController::new(Gains { proportional: 0.42, derivative: 0.0, neutral_duty_cycle: 0.5 });
        assert!((c.update(&ctx(0.5, 0.5)) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn pushes_harder_when_fish_above_bar() {
        let mut c = FishingController::default();
        let out = c.update(&ctx(0.9, 0.4));
        assert!(out > 0.5, "expected a stronger hold, got {out}");
    }

    #[test]
    fn output_is_always_clamped() {
        let mut c = FishingController::default();
        assert!((0.0..=1.0).contains(&c.update(&ctx(100.0, 0.0))));
        assert!((0.0..=1.0).contains(&c.update(&ctx(-100.0, 0.0))));
    }
}
