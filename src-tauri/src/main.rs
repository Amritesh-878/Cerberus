// Release builds on Windows open no console window beside the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cerberus_lib::run();
}
