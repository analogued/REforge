// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    match mach3::cli::run(std::env::args_os().skip(1)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("mach-3: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
