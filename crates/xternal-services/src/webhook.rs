// SPDX-License-Identifier: AGPL-3.0-only
//! Discord webhooks (port target: `Webhook.ahk` + `library/DiscordBuilder.ahk`).
//!
//! TODO: build the embed payload (session summary + instant alerts) and POST it
//! with `ureq`. `serde_json` replaces the vendored `JSON.ahk`/`DiscordBuilder`.
