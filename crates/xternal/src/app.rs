// SPDX-License-Identifier: AGPL-3.0-only
//! The egui application (port target: `ui/Gui.ahk`).
//!
//! Stubbed until `eframe`/`egui` are enabled. When you turn them on, make `App`
//! implement `eframe::App`, read [`Engine::status`] once per frame, and send
//! [`xternal_engine::Command`]s from the control buttons / hotkeys.

use xternal_engine::Engine;

pub struct App {
    engine: Engine,
}

impl App {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }

    /// Placeholder for the egui `update` body.
    fn draw_placeholder(&self) {
        let _status = self.engine.status();
        // TODO: render the status panel + dialogs with egui, themed via `ui::theme`.
    }
}
