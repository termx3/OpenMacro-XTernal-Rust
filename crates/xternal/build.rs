// SPDX-License-Identifier: AGPL-3.0-only
//! Build script for the XTernal binary.
//!
//! When you enable the `winresource` build-dependency, embed the app icon, a
//! Windows version resource, and an application manifest here. Keep the manifest
//! minimal: same-user `OpenProcess(PROCESS_VM_READ)` does NOT need elevation, so
//! do not request `requireAdministrator`.

fn main() {
    println!("cargo:rerun-if-changed=../../assets");

    // TODO (enable build-dep `winresource`):
    // if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
    //     let mut res = winresource::WindowsResource::new();
    //     res.set_icon("../../assets/icon.ico");
    //     res.compile().expect("failed to embed Windows resources");
    // }
}
