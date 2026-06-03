// SPDX-License-Identifier: AGPL-3.0-only
//! Live `MemoryReader` backed by `ReadProcessMemory`.

use xternal_engine::{GameState, MemoryReader, ReadError};

/// Reads game state from an attached `RobloxPlayerBeta.exe`.
///
/// In the scaffold this is a stub that reports "not attached". Wire the
/// `windows` crate's `OpenProcess` + `ReadProcessMemory` and the offset pointer
/// chains (see [`crate::process`], [`crate::instance`], [`crate::offsets`]) into
/// [`RobloxReader::snapshot`].
#[derive(Default)]
pub struct RobloxReader {
    // TODO: HANDLE to the process, module base address, resolved Offsets, and
    // the cached DataModel/LocalPlayer/Workspace instance pointers.
    attached: bool,
}

impl RobloxReader {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MemoryReader for RobloxReader {
    fn snapshot(&mut self) -> Result<GameState, ReadError> {
        // TODO: resolve DataModel → reel GUI → bar context through the offsets
        // and populate a real GameState. For now the engine just sees "detached".
        Ok(GameState { attached: self.attached, ..GameState::default() })
    }
}
