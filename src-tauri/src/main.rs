// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! The demo bin: the library's default cat, and nothing else.
//!
//! Everything it does is public API, so `Cargo.toml` pointing at another bin —
//! or an application depending on `rustcat-tauri` — can configure the same cat
//! without this file. See [`rustcat_tauri::RustCat`].

fn main() {
    if let Err(error) = rustcat_tauri::run() {
        // Before this point there is no logger to write to: the app's own log
        // plugin is installed inside `run`, so a failure to start has only
        // stderr. On Windows release builds the console is suppressed, and the
        // exit status is what is left of the reason.
        eprintln!("RustCat could not start: {error}");
        std::process::exit(1);
    }
}
