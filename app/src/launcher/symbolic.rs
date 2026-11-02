// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic initials independent of game IDs, fonts and original assets.
use unicode_segmentation::UnicodeSegmentation;

pub fn initials(title: &str) -> String {
    let mut words = title.unicode_words();
    let Some(first) = words.next() else {
        return "?".into();
    };
    let raw = if let Some(second) = words.next() {
        format!(
            "{}{}",
            first.graphemes(true).next().unwrap_or(""),
            second.graphemes(true).next().unwrap_or("")
        )
    } else {
        first.graphemes(true).take(2).collect()
    };
    let upper = raw.to_uppercase();
    upper.graphemes(true).take(2).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initials_cover_titles_numbers_punctuation_and_unicode() {
        for (name, expected) in [
            ("Alley Cat", "AC"),
            ("Mach 3", "M3"),
            ("Mindwheel", "MI"),
            ("Brimstone", "BR"),
            ("Essex", "ES"),
            ("R-Type", "RT"),
            ("King's Quest", "KQ"),
            ("Éclair", "ÉC"),
            ("e\u{301}clair", "E\u{301}C"),
            ("日本語", "日本"),
            ("7", "7"),
            ("", "?"),
            (" ✨ --- ", "?"),
        ] {
            assert_eq!(initials(name), expected, "{name}");
        }
    }
}
