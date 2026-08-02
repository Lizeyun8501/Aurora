// Aurora Desktop — Tauri v2 Rust entry placeholder.
//
// This file is a minimal placeholder so the `src-tauri/` cargo source tree
// exists. In a full Tauri v2 setup the `tauri-cli` generates the real entry
// (`tauri init`), and the JS plugin commands referenced by the TypeScript
// adapter (`app:set_tray`, `app:register_shortcut`, `app:set_menu`,
// `plugin:biometric|authenticate`, `plugin:clipboard|*`) are wired here via
// `#[tauri::command]` and `tauri::Builder::invoke_handler`.
//
// The frontend bundle is built from `apps/web` (see `tauri.conf.json`
// `build.frontendDist = "../web/dist"`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    aurora_desktop_lib::run()
}
