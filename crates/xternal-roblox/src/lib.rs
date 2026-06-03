// ============================================================================
//  OpenMacro XTernal (Rust)
//  SPDX-License-Identifier: AGPL-3.0-only
//  SPDX-FileCopyrightText: (c) 2026 OpenMacro XTernal (@anorexc)
//
//  Licensed under the GNU Affero General Public License, version 3.0 ONLY.
//  Full terms: see the LICENSE file in the project root, or
//  https://www.gnu.org/licenses/agpl-3.0.txt
// ============================================================================

//! Windows platform layer for XTernal: the live [`MemoryReader`] implementation
//! and everything that touches the Roblox process.
//!
//! This is the only crate that should depend on `windows` / Win32. Keeping the
//! `unsafe` `ReadProcessMemory` surface here lets the rest of the workspace stay
//! safe and testable.
//!
//! [`MemoryReader`]: xternal_engine::MemoryReader

// `read_ptr` and the offset pointer-chains assume 8-byte pointers. Modern Roblox
// is x64-only, so we make that a build-time guarantee rather than a 32-bit path
// we'd never run.
#[cfg(not(target_pointer_width = "64"))]
compile_error!("xternal-roblox assumes 8-byte pointers (x64-only Roblox)");

pub mod instance;
pub mod offsets;
pub mod process;
pub mod reader;

pub use reader::RobloxReader;
