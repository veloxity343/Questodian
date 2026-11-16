//! The desktop app. (On iOS, Tauri starts [`questodian_app_lib::run`] itself.)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    questodian_app_lib::run()
}
