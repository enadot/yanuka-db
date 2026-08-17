// Suppress the extra console window on Windows in release builds. This is a
// desktop application; a flashing terminal behind it looks broken.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    yanuka_desktop_lib::run()
}
