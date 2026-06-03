// SPDX-License-Identifier: AGPL-3.0-only
//! Self-updater (port target: `Update.ahk`).
//!
//! TODO: port the version check (GET `version.txt`), the GitHub tag-zip
//! download + install, and the post-update relaunch flow. The `self_update`
//! crate covers the "download a GitHub release and replace the running exe"
//! path; the existing archive-tag scheme maps onto its release backend.
