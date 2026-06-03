// SPDX-License-Identifier: AGPL-3.0-only
//! Live `MemoryReader` backed by `ReadProcessMemory`.

use thiserror::Error;
use xternal_engine::{GameState, MemoryReader, ReadError};

use crate::process::{self, ProcessError, ProcessHandle};

/// Why an attach-by-name request could not bind to exactly one process.
///
/// `attach(pid)` cannot produce `NotRunning`/`MultipleInstances` — those are
/// resolution outcomes that only exist when a *name* is the input. Keeping them
/// here makes "I asked for a name and got an ambiguous answer" a typed error the
/// consumer must handle, instead of a silent attach to an arbitrary instance.
#[derive(Debug, Error)]
pub enum AttachError {
    #[error("no process named {name:?} is running")]
    NotRunning { name: String },

    #[error("multiple processes named {name:?} are running ({pids:?}); pass an explicit PID")]
    MultipleInstances { name: String, pids: Vec<u32> },

    /// The PID was resolved, but opening it / reading its base address failed.
    #[error(transparent)]
    Process(#[from] ProcessError),
}

/// The exactly-one binding policy: a name resolves to a process only when the
/// match is unambiguous. Zero or many is an error, never a silent guess.
///
/// Pure over its inputs (no syscalls) so the policy is unit-tested directly;
/// the live PID enumeration that feeds it lives in [`crate::process`].
fn resolve_single(name: &str, pids: Vec<u32>) -> Result<u32, AttachError> {
    match pids.len() {
        0 => Err(AttachError::NotRunning { name: name.to_owned() }),
        1 => Ok(pids[0]),
        _ => Err(AttachError::MultipleInstances { name: name.to_owned(), pids }),
    }
}

/// Reads game state from an attached `RobloxPlayerBeta.exe`.
///
/// Holds the open process handle (and its resolved module base) once attached.
/// Reading the actual game state still needs the offset pointer chains (see
/// [`crate::instance`], [`crate::offsets`]) wired into [`RobloxReader::snapshot`].
#[derive(Default)]
pub struct RobloxReader {
    /// The attached process, or `None` until [`RobloxReader::attach`] succeeds.
    // TODO: resolved Offsets and the cached DataModel/LocalPlayer/Workspace
    // instance pointers live alongside this once instance reads land.
    handle: Option<ProcessHandle>,
}

impl RobloxReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach to a specific, caller-chosen PID.
    ///
    /// This is the primitive the consumer drives once it has decided *which*
    /// process to bind — the binding policy (which instance, when several alts
    /// run) belongs to the consumer, not this GUI-less library. Re-attaching
    /// drops any previously held handle.
    pub fn attach(&mut self, pid: u32) -> Result<(), ProcessError> {
        self.handle = Some(process::open(pid)?);
        Ok(())
    }

    /// Attach by process name, but only when the match is unambiguous.
    ///
    /// Zero matches → [`AttachError::NotRunning`]; more than one →
    /// [`AttachError::MultipleInstances`]. It never silently picks one of
    /// several, so a multi-instance situation surfaces as a typed error the
    /// consumer must resolve (by calling [`RobloxReader::attach`] with the PID
    /// it chooses) rather than a wrong-account attach.
    pub fn attach_by_name(&mut self, name: &str) -> Result<(), AttachError> {
        let pid = resolve_single(name, process::find_pids_by_name(name)?)?;
        self.attach(pid)?;
        Ok(())
    }

    /// Whether a process handle is currently held.
    pub fn is_attached(&self) -> bool {
        self.handle.is_some()
    }
}

impl MemoryReader for RobloxReader {
    fn snapshot(&mut self) -> Result<GameState, ReadError> {
        // TODO: resolve DataModel → reel GUI → bar context through the offsets
        // and populate a real GameState. For now the engine just reports whether
        // a process handle is held.
        Ok(GameState { attached: self.is_attached(), ..GameState::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_single_errors_when_no_process_matches() {
        let err = resolve_single("RobloxPlayerBeta.exe", vec![]).unwrap_err();
        assert!(matches!(err, AttachError::NotRunning { .. }));
    }

    #[test]
    fn resolve_single_returns_the_pid_when_exactly_one_matches() {
        assert_eq!(resolve_single("RobloxPlayerBeta.exe", vec![4242]).unwrap(), 4242);
    }

    #[test]
    fn resolve_single_errors_with_all_pids_when_multiple_match() {
        let err = resolve_single("RobloxPlayerBeta.exe", vec![10, 20, 30]).unwrap_err();
        match err {
            AttachError::MultipleInstances { pids, .. } => assert_eq!(pids, vec![10, 20, 30]),
            other => panic!("expected MultipleInstances, got {other:?}"),
        }
    }
}
