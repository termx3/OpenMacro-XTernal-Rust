// SPDX-License-Identifier: AGPL-3.0-only
//! Pointer offsets loaded from `resources/offsets.json` (port target: the
//! offsets handling in `Memory.ahk` / `OffsetsRemote.ahk`).

use std::collections::HashMap;

/// The flattened offset table, keyed by the legacy names used in the pointer
/// chains.
#[derive(Debug, Default, Clone)]
pub struct Offsets {
    pub roblox_version: String,
    pub entries: HashMap<String, i64>,
}

impl Offsets {
    /// Look up an offset by name.
    pub fn get(&self, name: &str) -> Option<i64> {
        self.entries.get(name).copied()
    }

    // TODO: `from_json(&str) -> Result<Offsets, _>` using serde_json, mirroring
    // ApplyParsedOffsets (read the "Offsets" section + "Roblox Version").
}
