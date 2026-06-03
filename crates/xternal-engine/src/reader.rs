// SPDX-License-Identifier: AGPL-3.0-only
//! The boundary between pure logic and the platform.
//!
//! `xternal-roblox` provides the live [`MemoryReader`] (backed by
//! `ReadProcessMemory`); tests provide a fake. The engine never sees a raw
//! pointer or a `DllCall`.

/// A snapshot of the slice of Roblox game state the macro cares about.
#[derive(Debug, Clone, Default)]
pub struct GameState {
    /// Whether the reader is currently attached to a live Roblox process.
    pub attached: bool,
    /// The reel minigame context, present only while the reel GUI is visible.
    pub reel: Option<ReelContext>,
    /// Fishing completion percentage (0.0..=100.0), when readable.
    pub completion_percent: Option<f64>,
    /// Name of the currently equipped rod, when readable.
    pub equipped_rod: Option<String>,
}

/// The reel bar context. Positions are normalised to `0.0..=1.0`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReelContext {
    pub fish_position: f64,
    pub bar_position: f64,
    pub bar_width: f64,
}

/// Everything the engine needs from the outside world.
///
/// `Send` so the live reader can be moved onto the engine thread.
pub trait MemoryReader: Send {
    /// Read the current game state, or `Err` if the process went away.
    fn snapshot(&mut self) -> Result<GameState, ReadError>;
}

/// Failure modes for a memory read. (Swap for a `thiserror` enum once enabled.)
#[derive(Debug, Clone)]
pub enum ReadError {
    /// The target process is no longer attachable.
    Detached,
    /// Any other read failure, with context.
    Other(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Detached => write!(f, "process detached"),
            ReadError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ReadError {}
