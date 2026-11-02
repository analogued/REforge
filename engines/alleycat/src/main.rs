// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
    if let Err(error) = alleycat::cli::run(std::env::args().skip(1)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
