// Copyright (c) 2026 analogued. Project license has not been selected.
#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    match mach3::cli::run(std::env::args_os().skip(1)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mach3-viewer: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
