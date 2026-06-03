// SPDX-License-Identifier: AGPL-3.0-only
//! Settings schema + load/migrate/persist (port target: `Constants.ahk` /
//! `Settings.ahk`).
//!
//! TODO: derive `serde::{Serialize, Deserialize}` on these structs and read/write
//! `%APPDATA%\OpenMacro\XTernal\settings.json` (resolve the dir with the
//! `directories` crate). The "add missing key" migration in `LoadSettings`
//! maps onto `#[serde(default)]` on each field.

/// The `main` settings block (a subset shown here for the scaffold).
#[derive(Debug, Clone)]
pub struct MainSettings {
    pub update_rate_ms: u64,
    pub proportional_gain: f64,
    pub derivative_gain: f64,
    pub completion_threshold: f64,
    pub cast_timeout_ms: u64,
}

impl Default for MainSettings {
    fn default() -> Self {
        Self {
            update_rate_ms: 21,
            proportional_gain: 0.42,
            derivative_gain: 0.55,
            completion_threshold: 99.7,
            cast_timeout_ms: 15_000,
        }
    }
}
