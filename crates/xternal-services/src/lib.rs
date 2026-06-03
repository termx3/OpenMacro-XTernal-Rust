// ============================================================================
//  OpenMacro XTernal (Rust)
//  SPDX-License-Identifier: AGPL-3.0-only
//  SPDX-FileCopyrightText: (c) 2026 OpenMacro XTernal (@anorexc)
//
//  Licensed under the GNU Affero General Public License, version 3.0 ONLY.
//  Full terms: see the LICENSE file in the project root, or
//  https://www.gnu.org/licenses/agpl-3.0.txt
// ============================================================================

//! Outward-facing services for XTernal — the slow, fallible I/O that is kept
//! out of the real-time control loop: settings persistence, the self-updater,
//! remote offsets fetching, and Discord webhooks.

pub mod offsets_remote;
pub mod settings;
pub mod update;
pub mod webhook;
