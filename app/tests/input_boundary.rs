// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("reforge-input-boundary-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        // Structurally readable data is not a supported release. An engine
        // parser must never be reached by normal play merely because it is MZ.
        let mut exe = vec![0; 55_067];
        exe[..2].copy_from_slice(b"MZ");
        exe[8..10].copy_from_slice(&0x20u16.to_le_bytes());
        fs::write(path.join("CAT.EXE"), exe).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn command_line_play_rejects_unregistered_data_before_engine_loading() {
    let fixture = Fixture::new();
    let original = fixture.0.join("CAT.EXE");
    let expected = reforge::detection::detect(&original).unwrap_err();
    let output = Command::new(env!("CARGO_BIN_EXE_reforge"))
        .arg("--original")
        .arg(&original)
        .arg("--no-config")
        .env("XDG_CONFIG_HOME", fixture.0.join("config"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains(&expected), "{error}");
    assert!(!error.contains("REforge: detected"), "{error}");
    assert!(!fixture.0.join("config").exists());

    #[cfg(feature = "dev-tools")]
    {
        let output = Command::new(env!("CARGO_BIN_EXE_btz-core"))
            .arg("--original")
            .arg(&original)
            .arg("--interpreter")
            .arg(&original)
            .arg("--snapshot")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(&expected), "{error}");
    }
}
