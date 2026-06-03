// SPDX-License-Identifier: AGPL-3.0-only
//! Remote offsets fetch + backup (port target: `OffsetsRemote.ahk`).
//!
//! TODO: GET the remote offsets JSON, validate it, back up the current
//! `resources/offsets.json`, then atomically replace it.
