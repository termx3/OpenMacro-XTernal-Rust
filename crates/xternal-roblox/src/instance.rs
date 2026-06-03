// SPDX-License-Identifier: AGPL-3.0-only
//! Roblox instance-tree traversal and DataModel caches (port target: the
//! primitive reads in `Read.ahk` and the cached accessors in `Memory.ahk`).
//!
//! TODO: implement `read_pointer` / `read_i32` / `read_f32` / `read_string`
//! over `ReadProcessMemory`, then `read_children` / `find_child_by_name` /
//! `find_child_by_class`, and cache DataModel/LocalPlayer/PlayerGui/Workspace.
