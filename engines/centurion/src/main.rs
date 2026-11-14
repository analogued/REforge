// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
fn main() {
    if let Err(message) = centurion::run_prototype() {
        eprintln!("centurion-prototype: {message}");
        std::process::exit(1);
    }
}
