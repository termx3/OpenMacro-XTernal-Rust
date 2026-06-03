// SPDX-License-Identifier: AGPL-3.0-only
//! Process attach (port target: `Process.ahk`).
//!
//! TODO: port the Win32 calls with the `windows` crate —
//!   * find `RobloxPlayerBeta.exe` (toolhelp snapshot or process enumeration),
//!   * `OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, ...)`,
//!   * `K32EnumProcessModulesEx` for the module base address,
//!   * `QueryFullProcessImageNameW` → extract the `version-<hash>` string.
//!
//! Note: reading another same-user process needs no elevation, so the app
//! manifest must NOT request administrator rights.
