// SPDX-License-Identifier: AGPL-3.0-only
//! The engine loop: owns the [`MemoryReader`], runs the control tick on its own
//! thread, and publishes a [`Status`] snapshot the UI thread reads each repaint.
//!
//! This replaces `Main.ahk`'s single-threaded `SetTimer(MacroLoop, 21)`: the
//! real-time loop no longer shares a thread with GUI redraws.

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::controller::FishingController;
use crate::reader::MemoryReader;
use crate::state::Phase;

/// Commands the UI sends to the engine thread (Start/Stop/Fix/Reload hotkeys).
#[derive(Debug, Clone)]
pub enum Command {
    Start,
    Stop,
    Shutdown,
}

/// The snapshot the UI reads. Cheap to clone; refreshed every tick.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub running: bool,
    pub attached: bool,
    pub phase: Phase,
    pub fish_caught: u64,
    pub fish_lost: u64,
    pub last_error: Option<String>,
}

/// Handle to a running engine thread. Dropping it shuts the thread down cleanly.
pub struct Engine {
    tx: Sender<Command>,
    status: Arc<Mutex<Status>>,
    handle: Option<JoinHandle<()>>,
}

impl Engine {
    /// Spawn the engine on its own thread, ticking at `tick` (the AHK
    /// `update_rate`, default 21 ms).
    pub fn spawn(reader: Box<dyn MemoryReader>, tick: Duration) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<Command>();
        let status = Arc::new(Mutex::new(Status::default()));
        let status_for_thread = Arc::clone(&status);

        let handle = thread::spawn(move || {
            run_loop(reader, rx, status_for_thread, tick);
        });

        Self { tx, status, handle: Some(handle) }
    }

    /// Send a command to the engine thread.
    pub fn command(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    /// Cheap snapshot read for the UI thread.
    pub fn status(&self) -> Status {
        self.status.lock().expect("status mutex poisoned").clone()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_loop(
    mut reader: Box<dyn MemoryReader>,
    rx: Receiver<Command>,
    status: Arc<Mutex<Status>>,
    tick: Duration,
) {
    let mut running = false;
    let mut controller = FishingController::default();

    loop {
        // Drain every pending command before this tick's work.
        loop {
            match rx.try_recv() {
                Ok(Command::Start) => running = true,
                Ok(Command::Stop) => {
                    running = false;
                    controller.reset();
                }
                Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }

        if running {
            match reader.snapshot() {
                Ok(state) => {
                    // TODO: drive the phase machine (fishing/totem/appraise) and
                    // feed `controller` from `state.reel`. Stubbed for now so the
                    // scaffold compiles and runs end to end.
                    let mut s = status.lock().expect("status mutex poisoned");
                    s.running = true;
                    s.attached = state.attached;
                    s.last_error = None;
                }
                Err(err) => {
                    let mut s = status.lock().expect("status mutex poisoned");
                    s.attached = false;
                    s.last_error = Some(err.to_string());
                }
            }
        } else {
            let mut s = status.lock().expect("status mutex poisoned");
            s.running = false;
            s.phase = Phase::Off;
        }

        thread::sleep(tick);
    }
}
