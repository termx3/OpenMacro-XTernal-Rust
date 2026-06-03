// SPDX-License-Identifier: AGPL-3.0-only
//! Macro phase state machine — mirrors the CAST → CASTED → SHAKE → FISHING flow
//! from `Fish.ahk`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Off,
    Cast,
    Casted,
    Shake,
    Fishing,
    Tranquility,
}

impl Phase {
    /// Human-readable label for the status panel.
    pub fn label(self) -> &'static str {
        match self {
            Phase::Off => "Off",
            Phase::Cast => "Casting",
            Phase::Casted => "Casted",
            Phase::Shake => "Shaking",
            Phase::Fishing => "Fishing",
            Phase::Tranquility => "Tranquility",
        }
    }
}
