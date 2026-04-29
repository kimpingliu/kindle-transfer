//! Desktop binary entrypoint.
//!
//! The UI can evolve independently from the Rust domain modules, but Tauri
//! still needs a thin executable that boots the shared backend library.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    kindle_transfer_lib::run();
}
