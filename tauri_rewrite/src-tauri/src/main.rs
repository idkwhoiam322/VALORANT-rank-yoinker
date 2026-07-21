// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Limit tokio worker threads to reduce idle memory (each thread reserves a 2MB stack).
    // The app is I/O-bound (HTTP polling + WebSocket), 2 threads is sufficient.
    std::env::set_var("TOKIO_WORKER_THREADS", "2");
    vry_rust_lib::run()
}
