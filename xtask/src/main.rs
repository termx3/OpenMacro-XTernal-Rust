// SPDX-License-Identifier: AGPL-3.0-only
//! Workspace automation entry point — the cargo-xtask pattern (build/package/
//! release steps live in Rust instead of a Makefile or .ps1).
//!
//! Run via `cargo xtask <task>` (alias defined in `.cargo/config.toml`).

fn main() {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        Some("dist") => {
            println!("TODO: cargo build --release, embed resources, zip a portable build.");
        }
        Some("offsets") => {
            println!("TODO: fetch the latest offsets.json and update resources/offsets.json.");
        }
        other => {
            if let Some(name) = other {
                eprintln!("unknown task: {name}\n");
            }
            eprintln!("xtask — available tasks:");
            eprintln!("  dist     package a release build");
            eprintln!("  offsets  refresh resources/offsets.json");
            std::process::exit(2);
        }
    }
}
