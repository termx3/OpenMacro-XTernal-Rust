// SPDX-License-Identifier: AGPL-3.0-only
//! Process attach: open `RobloxPlayerBeta.exe` and resolve its main-module
//! base address (port target: `Process.ahk` → `GetProcessBase`).

use core::ffi::c_void;
use std::mem;

use thiserror::Error;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, GetLastError, HANDLE, HMODULE,
};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{
    K32EnumProcessModulesEx, LIST_MODULES_ALL,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
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

// ── memory-read error ──────────────────────────────────────────────────────

/// Failure modes for a `ReadProcessMemory` call, carrying enough context for the
/// offset healer to act on. Two variants, deliberately:
///
/// - [`MemError::Detached`] — the target is gone; abort, do not heal.
/// - [`MemError::ReadFailed`] — the read failed while the process is alive
///   (bad pointer / wrong offset / page straddle); the healer keeps probing.
///
/// A null pointer is intentionally *not* a variant: this primitive reports raw
/// I/O truth, and the instance-aware readers in `instance.rs` map a null pointer
/// to a domain value before they ever issue a read.
#[derive(Debug, Error)]
pub enum MemError {
    #[error("process detached")]
    Detached,

    #[error("read of {requested} bytes at {addr:#x} failed after {read} (Win32 {win32})")]
    ReadFailed {
        addr: usize,
        requested: usize,
        read: usize,
        /// The *bare* Win32 error code (e.g. 299 = `ERROR_PARTIAL_COPY`), taken
        /// from `GetLastError` — NOT the HRESULT-wrapped form that the windows
        /// crate's `Error::code()` would give.
        win32: u32,
    },
}

impl From<MemError> for xternal_engine::ReadError {
    /// Collapse to the engine's platform-agnostic error at the `snapshot`
    /// boundary. The Detached/ReadFailed *distinction* survives (Detached →
    /// Detached); only the structured fields drop, since the engine never heals
    /// and so never needs them.
    fn from(e: MemError) -> Self {
        match e {
            MemError::Detached => xternal_engine::ReadError::Detached,
            other => xternal_engine::ReadError::Other(other.to_string()),
        }
    }
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

// ── memory reads (port target: Read.ahk primitives) ──────────────────────────

impl ProcessHandle {
    /// The sole `ReadProcessMemory` call site: copy exactly `buf.len()` bytes
    /// from `addr` in the target into `buf`, or fail. This is the allocation-free
    /// primitive for the polling hot path — every other reader delegates here.
    pub fn read_into(&self, addr: usize, buf: &mut [u8]) -> Result<(), MemError> {
        let mut read = 0usize;

        // SAFETY: FFI. `self.raw` is a live handle opened with PROCESS_VM_READ.
        // We pass `buf.len()` as the count and `buf` is exactly that long, so RPM
        // cannot write past it.
        let ok = unsafe {
            ReadProcessMemory(
                self.raw,
                addr as *const c_void,
                buf.as_mut_ptr().cast::<c_void>(),
                buf.len(),
                Some(&mut read),
            )
        };

        if ok.is_ok() && read == buf.len() {
            return Ok(());
        }

        // Capture the BARE Win32 code now, before classify()'s liveness probe can
        // overwrite the thread's last-error.
        let win32 = unsafe { GetLastError().0 };
        Err(self.classify(addr, buf.len(), read, win32))
    }

    /// Fixed-width read into a stack buffer (zero allocation). Const-generic over
    /// the width so the typed readers below get a `[u8; N]` straight to
    /// `from_le_bytes`. Delegates to [`read_into`] so the `unsafe` stays in one
    /// place.
    fn read_raw<const N: usize>(&self, addr: usize) -> Result<[u8; N], MemError> {
        let mut buf = [0u8; N];
        self.read_into(addr, &mut buf)?;
        Ok(buf)
    }

    /// Classify a failed read: process gone (abort) vs. failed-while-alive (heal).
    fn classify(&self, addr: usize, requested: usize, read: usize, win32: u32) -> MemError {
        if self.is_alive() {
            MemError::ReadFailed { addr, requested, read, win32 }
        } else {
            MemError::Detached
        }
    }

    /// Whether the target is still running, via `GetExitCodeProcess` — our handle
    /// carries QUERY rights but not `SYNCHRONIZE`, so `WaitForSingleObject` is
    /// out. Runs only on the read error path, so it never taxes the happy path.
    /// A process that genuinely exits with code 259 reads as alive (rare; the
    /// read still fails, so it just lands in `ReadFailed` anyway).
    fn is_alive(&self) -> bool {
        // STATUS_PENDING: GetExitCodeProcess's "still running" sentinel.
        const STILL_ACTIVE: u32 = 259;
        let mut code = 0u32;
        // SAFETY: FFI; `self.raw` carries PROCESS_QUERY_(LIMITED_)INFORMATION.
        match unsafe { GetExitCodeProcess(self.raw, &mut code) } {
            Ok(()) => code == STILL_ACTIVE,
            // Query failed — don't mask a real read failure as Detached.
            Err(_) => true,
        }
    }

    /// Read an unsigned byte.
    pub fn read_u8(&self, addr: usize) -> Result<u8, MemError> {
        Ok(self.read_raw::<1>(addr)?[0])
    }

    /// Read a little-endian `i32`. Explicit LE (not `from_ne_bytes`): correct by
    /// intent, even though the only target is little-endian x64.
    pub fn read_i32(&self, addr: usize) -> Result<i32, MemError> {
        Ok(i32::from_le_bytes(self.read_raw::<4>(addr)?))
    }

    /// Read a little-endian `f32`.
    pub fn read_f32(&self, addr: usize) -> Result<f32, MemError> {
        Ok(f32::from_le_bytes(self.read_raw::<4>(addr)?))
    }

    /// Read a little-endian `f64`.
    pub fn read_f64(&self, addr: usize) -> Result<f64, MemError> {
        Ok(f64::from_le_bytes(self.read_raw::<8>(addr)?))
    }

    /// Read an 8-byte pointer as `usize` (x64 only — see the crate-root guard).
    pub fn read_ptr(&self, addr: usize) -> Result<usize, MemError> {
        Ok(u64::from_le_bytes(self.read_raw::<8>(addr)?) as usize)
    }

    /// Allocate-and-read convenience over [`read_into`].
    pub fn read_bytes(&self, addr: usize, len: usize) -> Result<Vec<u8>, MemError> {
        let mut v = vec![0u8; len];
        self.read_into(addr, &mut v)?;
        Ok(v)
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

    // ── read primitives ──────────────────────────────────────────────────────
    //
    // Every read test runs against our OWN process: deterministic, CI-safe, and
    // it exercises the real ReadProcessMemory path end-to-end.

    use std::process::Command;

    #[test]
    fn typed_readers_decode_each_width_from_our_own_memory() {
        #[repr(C)]
        struct Probe {
            a: u8,
            b: i32,
            c: f32,
            d: f64,
            e: u64,
        }
        // black_box so the locals can't be optimized into registers (no address).
        let probe = std::hint::black_box(Probe {
            a: 0xAB,
            b: -123_456,
            c: 1.5,
            d: 2.5,
            e: 0x8000_0000_0000_1234, // high bit set: proves read_ptr reads 8 bytes, no truncation
        });
        let h = open(std::process::id()).unwrap();

        assert_eq!(h.read_u8(core::ptr::addr_of!(probe.a) as usize).unwrap(), 0xAB);
        assert_eq!(h.read_i32(core::ptr::addr_of!(probe.b) as usize).unwrap(), -123_456);
        assert_eq!(h.read_f32(core::ptr::addr_of!(probe.c) as usize).unwrap(), 1.5);
        assert_eq!(h.read_f64(core::ptr::addr_of!(probe.d) as usize).unwrap(), 2.5);
        assert_eq!(
            h.read_ptr(core::ptr::addr_of!(probe.e) as usize).unwrap(),
            0x8000_0000_0000_1234,
        );
    }

    #[test]
    fn read_bytes_and_read_into_copy_exact_slices() {
        let data = std::hint::black_box([1u8, 2, 3, 4, 5]);
        let h = open(std::process::id()).unwrap();
        let addr = data.as_ptr() as usize;

        assert_eq!(h.read_bytes(addr, 5).unwrap(), vec![1, 2, 3, 4, 5]);

        let mut buf = [0u8; 3];
        h.read_into(addr, &mut buf).unwrap();
        assert_eq!(buf, [1, 2, 3]);
    }

    #[test]
    fn reading_unmapped_memory_errors_rather_than_returning_zero() {
        let h = open(std::process::id()).unwrap();
        // The null page is never mapped — a faithful read MUST fail here, not
        // silently yield 0 (the AHK sentinel behavior this layer is designed out of).
        match h.read_ptr(0) {
            Err(MemError::ReadFailed { addr, read, .. }) => {
                assert_eq!(addr, 0);
                assert_eq!(read, 0);
            }
            other => panic!("expected ReadFailed, got {other:?}"),
        }
    }

    #[test]
    fn read_after_process_exit_reports_detached() {
        let mut child = Command::new("cmd")
            .args(["/C", "ping -n 6 127.0.0.1 >NUL"])
            .spawn()
            .expect("spawn child");

        // Build the handle via try_open directly: we only need a VM-read handle
        // + PID to exercise the read/classify path, NOT the module-base
        // resolution open() does — that races a just-spawned child's module
        // loading (EnumProcessModulesEx → ERROR_PARTIAL_COPY) and is irrelevant here.
        let raw = try_open(child.id()).expect("open child");
        let handle = ProcessHandle { raw, pid: child.id(), base_address: 0 };

        child.kill().expect("kill child");
        child.wait().expect("reap child");

        // Process is gone: classify must report Detached, not ReadFailed.
        match handle.read_i32(0x10_0000) {
            Err(MemError::Detached) => {}
            other => panic!("expected Detached, got {other:?}"),
        }
    }

    #[test]
    fn memerror_detached_maps_to_engine_detached() {
        let mapped: xternal_engine::ReadError = MemError::Detached.into();
        assert!(matches!(mapped, xternal_engine::ReadError::Detached));
    }

    #[test]
    fn memerror_readfailed_maps_to_engine_other() {
        let mapped: xternal_engine::ReadError = MemError::ReadFailed {
            addr: 0x10,
            requested: 4,
            read: 0,
            win32: 299,
        }
        .into();
        assert!(matches!(mapped, xternal_engine::ReadError::Other(_)));
    }
}