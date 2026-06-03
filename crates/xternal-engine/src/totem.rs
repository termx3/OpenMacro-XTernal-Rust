// SPDX-License-Identifier: AGPL-3.0-only
//! Auto-totem workflow (port target: the `UpdateAutoTotem*` functions in
//! `Fish.ahk` plus the hotbar/world-config reads in `Totem.ahk`).
//!
//! TODO: model the totem state machine (due → equip → use → confirm) over
//! [`crate::reader::GameState`], keeping all timing pure and testable.
