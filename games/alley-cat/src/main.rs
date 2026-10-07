// Copyright (c) 2026 analogued. Project license has not yet been selected.

fn main() {
    if let Err(error) = alley_cat::cli::run(std::env::args().skip(1)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
