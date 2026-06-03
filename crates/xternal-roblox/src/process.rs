// SPDX-License-Identifier: AGPL-3.0-only
//! Process attach: open `RobloxPlayerBeta.exe` and resolve its main-module
//! base address (port target: `Process.ahk` → `GetProcessBase`).

use std::mem;

use thiserror::Error;
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HMODULE};
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