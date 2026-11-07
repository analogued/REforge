// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() -> std::process::ExitCode {
    match reforge::run(std::env::args_os().skip(1)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("REforge: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
