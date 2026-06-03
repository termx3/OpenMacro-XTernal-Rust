// SPDX-License-Identifier: AGPL-3.0-only
//! Color themes (port target: `GetBuiltInThemes` in `Constants.ahk`).
//!
//! Colors are packed `0xRRGGBB`. Convert to egui's `Color32` once the GUI is
//! wired.

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub accent: u32,
    pub bg: u32,
    pub text: u32,
    pub border: u32,
}

impl Theme {
    pub const DEFAULT: Theme =
        Theme { accent: 0x5aa9ff, bg: 0x0f1115, text: 0xf5f7fa, border: 0x2a2f3a };
    pub const CRIMSON: Theme =
        Theme { accent: 0xff4c4c, bg: 0x1a0a0a, text: 0xf5e6e6, border: 0x3a1f1f };
    pub const EMERALD: Theme =
        Theme { accent: 0x3ddfa0, bg: 0x0a1512, text: 0xe6f5ef, border: 0x1f3a2d };

    /// The built-in palette, by name (the rest port over from `GetBuiltInThemes`).
    pub fn builtin() -> &'static [(&'static str, Theme)] {
        &[
            ("Default", Theme::DEFAULT),
            ("Crimson", Theme::CRIMSON),
            ("Emerald", Theme::EMERALD),
        ]
    }
}
