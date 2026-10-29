#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;
mod win;

fn main() -> std::process::ExitCode {
    if let Err(error) = win::run() {
        eprintln!("{error}");
        win::show_error(&error);
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
