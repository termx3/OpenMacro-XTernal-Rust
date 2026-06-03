// SPDX-License-Identifier: AGPL-3.0-only
//! Fishing phase logic (port target: `Fish.ahk`'s phase handlers).
//!
//! TODO: port `UpdateCastingPhase` / `UpdateCastedPhase` / `UpdateShakePhase` /
//! `UpdateFishingPhase` here. Each consumes a [`crate::reader::GameState`] and
//! emits input actions through an injected sink, advancing [`crate::state::Phase`].
