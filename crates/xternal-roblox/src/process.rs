// SPDX-License-Identifier: AGPL-3.0-only
//! Process attach: open `RobloxPlayerBeta.exe` and resolve its main-module
//! base address (port target: `Process.ahk` → `GetProcessBase`).

use std::mem;

use thiserror::Error;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, GetLastError, HANDLE, HMODULE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    K32EnumProcessModulesEx, LIST_MODULES_ALL,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_VM_READ,
};

pub const ROBLOX_PROCESS_NAME: &str = "RobloxPlayerBeta.exe";

// ── error type ───────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("could not open process {pid} (Win32 {win32:#010x})")]
    OpenFailed { pid: u32, win32: u32 },

    #[error("K32EnumProcessModulesEx failed for process {pid} (Win32 {win32:#010x})")]
    EnumModulesFailed { pid: u32, win32: u32 },

    #[error("process {pid} has no loaded modules")]
    NoModules { pid: u32 },

    #[error("CreateToolhelp32Snapshot failed (Win32 {win32:#010x})")]
    SnapshotFailed { win32: u32 },

    #[error("process-list enumeration failed (Win32 {win32:#010x})")]
    EnumProcessesFailed { win32: u32 },
}

// ── handle type ──────────────────────────────────────────────────────────────

/// RAII owner of a Win32 process handle.
///
/// The underlying handle is closed automatically on drop — callers must never
/// call `CloseHandle` directly on the value from [`open`]. This replaces the
/// `H_PROCESS` global in `Process.ahk` and eliminates its leak-on-error paths.
pub struct ProcessHandle {
    raw: HANDLE,
    pub pid: u32,
    /// Virtual load address of the main `.exe` module.
    ///
    /// All pointer-chain arithmetic from `offsets.json` is applied relative to
    /// this value — it is the Rust equivalent of `RBLX_BASE` in `Constants.ahk`.
    pub base_address: usize,
}

impl ProcessHandle {
    /// The raw Win32 handle — needed for `ReadProcessMemory` calls.
    #[inline]
    pub fn raw(&self) -> HANDLE {
        self.raw
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: `raw` was returned by `OpenProcess` and has not been closed.
        unsafe { let _ = CloseHandle(self.raw); }
    }
}

// SAFETY: a Win32 process handle is a process-wide kernel object, not tied to
// the thread that opened it. `ReadProcessMemory` and `CloseHandle` are valid
// from any thread, so moving ownership across threads is sound. The engine
// requires `MemoryReader: Send` precisely so the live reader (which owns one of
// these) can run on its own thread.
unsafe impl Send for ProcessHandle {}

// ── public API ───────────────────────────────────────────────────────────────

/// Open a process by PID with VM-read rights and resolve its main-module base
/// address.
///
/// # Access strategy (mirrors `GetProcessBase` in `Process.ahk`)
///
/// 1. `PROCESS_QUERY_INFORMATION | PROCESS_VM_READ` — full query, always tried
///    first.
/// 2. `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ` — restricted fallback
///    that succeeds even when the caller lacks `SeDebugPrivilege` or the target
///    has a tighter DACL.
///
/// Note: reading another same-user process at normal integrity needs *no*
/// elevation — do NOT add an administrator manifest entry.
///
/// # Base address
///
/// `K32EnumProcessModulesEx(LIST_MODULES_ALL)` enumerates every module loaded
/// into the process.  The first slot is always the main executable.  On Windows,
/// `HMODULE` **is** the module's virtual base address (they are the same value),
/// so casting it to `usize` gives the anchor for all subsequent pointer
/// arithmetic — equivalent to AHK's `NumGet(hMods, 0, "UPtr")`.
pub fn open(pid: u32) -> Result<ProcessHandle, ProcessError> {
    let raw = try_open(pid)?;
    let base_address = resolve_base(raw, pid)?;
    Ok(ProcessHandle { raw, pid, base_address })
}

/// Enumerate the PIDs of every running process whose image name matches `name`
/// (case-insensitive), via a Toolhelp process snapshot.
///
/// This is the primitive that replaces AHK's `ProcessExist` — but it returns
/// *all* matches rather than the first, because Toolhelp enumeration order is
/// not a documented contract (it is not launch order). With multiple Roblox
/// instances running, "first match" would attach to a semi-arbitrary one; the
/// `Vec` forces the caller to disambiguate instead of silently guessing.
///
/// Three honest outcomes:
/// - `Ok(vec![])` — no such process is running (normal, not an error).
/// - `Ok(vec![..])` — one or more matches.
/// - `Err(_)` — the lookup machinery itself failed (bad snapshot / enumeration).
pub fn find_pids_by_name(name: &str) -> Result<Vec<u32>, ProcessError> {
    // SAFETY: FFI. The snapshot is a kernel handle closed by the guard below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|e| ProcessError::SnapshotFailed { win32: e.code().0 as u32 })?;
    let _guard = SnapshotGuard(snapshot);

    let mut entry = PROCESSENTRY32W {
        // Required by the API: the struct must announce its own size before the
        // first call, or Process32FirstW rejects it.
        dwSize: mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    // Seed the walk. A failure here is a genuine error (the list is never empty
    // on a live system, so this is not the end-of-enumeration signal).
    unsafe { Process32FirstW(snapshot, &mut entry) }
        .map_err(|e| ProcessError::EnumProcessesFailed { win32: e.code().0 as u32 })?;

    let mut pids = Vec::new();
    loop {
        if exe_name_matches(&entry.szExeFile, name) {
            pids.push(entry.th32ProcessID);
        }

        // Advance. Toolhelp signals "end of list" by *failing* with
        // ERROR_NO_MORE_FILES — that is the clean terminator, NOT an error.
        // Any other failure code is a real enumeration error worth propagating,
        // which is the whole reason this returns Result and not a bare Vec.
        match unsafe { Process32NextW(snapshot, &mut entry) } {
            Ok(()) => continue,
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(e) => {
                return Err(ProcessError::EnumProcessesFailed { win32: e.code().0 as u32 });
            }
        }
    }

    Ok(pids)
}

/// Convenience over [`find_pids_by_name`] for the single-instance case: returns
/// the first match, or `Ok(None)` if none are running.
///
/// This deliberately does *not* disambiguate — with several instances it returns
/// an arbitrary one. The attach path must not use it for that reason; use
/// `RobloxReader::attach_by_name`, which makes "more than one" a typed error.
pub fn find_pid_by_name(name: &str) -> Result<Option<u32>, ProcessError> {
    Ok(find_pids_by_name(name)?.into_iter().next())
}

// ── internals ────────────────────────────────────────────────────────────────

/// Try `PROCESS_QUERY_INFORMATION` first, then the limited fallback.
///
/// The windows crate wraps the Win32 error into `windows_core::Error`, so the
/// error code is available without a separate `GetLastError` call.
fn try_open(pid: u32) -> Result<HANDLE, ProcessError> {
    let result = unsafe {
        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
            .or_else(|_| {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                    false,
                    pid,
                )
            })
    };

    result.map_err(|e| ProcessError::OpenFailed {
        pid,
        win32: e.code().0 as u32,
    })
}

/// Call `K32EnumProcessModulesEx` and extract the main-module base address.
///
/// The AHK original allocates a fixed 1024-slot buffer and silently truncates if
/// Roblox loads more modules.  Here we do a single grow-and-retry if the initial
/// buffer is too small, which is the documented correct usage of `cbNeeded`.
fn resolve_base(handle: HANDLE, pid: u32) -> Result<usize, ProcessError> {
    const INITIAL_SLOTS: usize = 1024;

    let mut modules = vec![HMODULE::default(); INITIAL_SLOTS];
    let mut needed_bytes: u32 = 0;

    // First call — sufficient for every real Roblox launch, but sized correctly.
    // K32EnumProcessModulesEx returns BOOL (not windows_core::Result), so we
    // call GetLastError() ourselves on failure.
    let ok = unsafe {
        K32EnumProcessModulesEx(
            handle,
            modules.as_mut_ptr(),
            slot_bytes(&modules),
            &mut needed_bytes,
            LIST_MODULES_ALL.0, // unwrap ENUM_PROCESS_MODULES_EX_FLAGS → u32
        )
    };

    if !ok.as_bool() {
        return Err(ProcessError::EnumModulesFailed {
            pid,
            win32: unsafe { GetLastError().0 },
        });
    }

    // `cbNeeded` reports the total bytes required. If the buffer was too small,
    // resize to exactly the right count and retry once.
    let needed_slots = needed_bytes as usize / mem::size_of::<HMODULE>();
    if needed_slots > modules.len() {
        modules.resize(needed_slots, HMODULE::default());

        let ok = unsafe {
            K32EnumProcessModulesEx(
                handle,
                modules.as_mut_ptr(),
                slot_bytes(&modules),
                &mut needed_bytes,
                LIST_MODULES_ALL.0,
            )
        };

        if !ok.as_bool() {
            return Err(ProcessError::EnumModulesFailed {
                pid,
                win32: unsafe { GetLastError().0 },
            });
        }
    }

    // Slot 0 is always the main executable module. HMODULE.0 is *mut c_void;
    // casting through usize gives the virtual base address for pointer arithmetic.
    modules
        .first()
        .filter(|m| !m.0.is_null())
        .map(|m| m.0 as usize)
        .ok_or(ProcessError::NoModules { pid })
}

/// Byte length of a module-handle slice, as `u32` for the Win32 API.
#[inline]
fn slot_bytes(modules: &[HMODULE]) -> u32 {
    (modules.len() * mem::size_of::<HMODULE>()) as u32
}

/// RAII closer for the Toolhelp snapshot handle, so every exit path from
/// [`find_pids_by_name`] (including the early `?` on enumeration failure) closes
/// it. Unlike the AHK original, no leak-on-error path exists.
struct SnapshotGuard(HANDLE);

impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        // SAFETY: `0` came from a successful CreateToolhelp32Snapshot and is
        // closed exactly once, here.
        unsafe { let _ = CloseHandle(self.0); }
    }
}

/// Case-insensitive compare of a `PROCESSENTRY32W::szExeFile` field (a
/// NUL-padded UTF-16 array) against `name`. Process names are ASCII, so an
/// ASCII-case-insensitive match is correct and avoids Unicode-folding cost.
fn exe_name_matches(sz_exe_file: &[u16], name: &str) -> bool {
    let end = sz_exe_file.iter().position(|&c| c == 0).unwrap_or(sz_exe_file.len());
    String::from_utf16_lossy(&sz_exe_file[..end]).eq_ignore_ascii_case(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a NUL-padded UTF-16 `szExeFile`-style buffer from a string.
    fn sz(s: &str) -> [u16; 260] {
        let mut buf = [0u16; 260];
        for (i, unit) in s.encode_utf16().enumerate() {
            buf[i] = unit;
        }
        buf
    }

    #[test]
    fn exe_name_matches_is_case_insensitive_and_stops_at_nul() {
        // The trailing NUL padding must NOT bleed into the comparison — a
        // positive match here proves the NUL terminator is honored.
        let buf = sz("RobloxPlayerBeta.exe");
        assert!(exe_name_matches(&buf, "robloxplayerbeta.exe"));
        assert!(exe_name_matches(&buf, "ROBLOXPLAYERBETA.EXE"));
    }

    #[test]
    fn exe_name_matches_rejects_a_different_name() {
        assert!(!exe_name_matches(&sz("notepad.exe"), ROBLOX_PROCESS_NAME));
    }

    /// Smoke test for the live FFI path: enumerating for a name that cannot
    /// exist must walk the entire snapshot, hit ERROR_NO_MORE_FILES, and return
    /// `Ok(empty)` — not an `Err`. If the terminator were mishandled (treated as
    /// a failure), this `unwrap` would panic.
    #[test]
    fn find_pids_by_name_returns_empty_for_a_nonexistent_process() {
        let pids = find_pids_by_name("definitely-not-real-7f3a9c.exe").unwrap();
        assert!(pids.is_empty());
    }
}