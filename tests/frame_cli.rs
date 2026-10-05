// Copyright (c) 2026 analogued. Project license has not been selected.
//! CLI behavior uses only authored synthetic resources.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("mach3-frame-test-{}-{suffix}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn render(&self, output: &str, format: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mach3-frame"))
            .args(["--format", format])
            .arg(&self.0)
            .arg(self.0.join(output))
            .output()
            .unwrap()
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn sprite_file(pixel: u8) -> Vec<u8> {
    let mut file = vec![0; 0x200];
    for index in 0..190 {
        let pointer = u16::try_from(0x2000 + file.len() - 0x200).unwrap();
        file[index * 2..index * 2 + 2].copy_from_slice(&pointer.to_le_bytes());
        file.extend_from_slice(&[0, 0, 0, 14, 18, 0, 0]);
        file.extend_from_slice(&[pixel; 14 * 18]);
    }
    file
}

#[test]
fn renders_both_formats_and_protects_existing_files() {
    let directory = TestDirectory::new();
    fs::write(directory.0.join("data2"), vec![0; 28_960]).unwrap();
    fs::write(directory.0.join("data3"), vec![0; 16_000]).unwrap();
    fs::write(directory.0.join("data4"), sprite_file(0)).unwrap();
    fs::write(directory.0.join("data5"), sprite_file(255)).unwrap();

    let result = directory.render("frame.ppm", "ppm");
    assert!(result.status.success(), "{:?}", result);
    let ppm = fs::read(directory.0.join("frame.ppm")).unwrap();
    assert!(ppm.starts_with(b"P6\n320 200\n255\n"));
    assert_eq!(ppm.len(), 15 + 320 * 200 * 3);
    assert!(ppm[15..].iter().all(|byte| *byte == 0));
    assert!(!directory.render("frame.ppm", "ppm").status.success());
    assert_eq!(fs::read(directory.0.join("frame.ppm")).unwrap(), ppm);

    assert!(directory.render("frame.bin", "cga").status.success());
    assert_eq!(
        fs::read(directory.0.join("frame.bin")).unwrap(),
        vec![0; 0x4000]
    );
    assert!(!directory.render("data2", "ppm").status.success());
    assert_eq!(
        fs::read(directory.0.join("data2")).unwrap(),
        vec![0; 28_960]
    );
    assert!(!directory.render("invalid.bin", "unknown").status.success());
    assert!(!directory.0.join("invalid.bin").exists());
}

#[test]
fn opening_export_preserves_existing_directories_and_rejects_unsupported_steps() {
    let directory = TestDirectory::new();
    fs::write(directory.0.join("data2"), vec![0; 28_960]).unwrap();
    fs::write(directory.0.join("data3"), vec![0; 16_000]).unwrap();
    fs::write(directory.0.join("data4"), sprite_file(0)).unwrap();
    fs::write(directory.0.join("data5"), sprite_file(255)).unwrap();
    let export = |steps: &str, destination: &str| {
        Command::new(env!("CARGO_BIN_EXE_mach3-opening"))
            .args(["--steps", steps])
            .arg(&directory.0)
            .arg(directory.0.join(destination))
            .output()
            .unwrap()
    };
    let result = export("2", "opening");
    assert!(result.status.success(), "{result:?}");
    let first = fs::read(directory.0.join("opening/000.cga")).unwrap();
    assert_eq!(first, vec![0; 0x4000]);
    assert_eq!(
        fs::metadata(directory.0.join("opening/002.cga"))
            .unwrap()
            .len(),
        0x4000
    );
    assert!(!directory.0.join("opening/003.cga").exists());
    let state = fs::read_to_string(directory.0.join("opening/state.tsv")).unwrap();
    assert_eq!(state.lines().count(), 4);
    assert!(!export("1", "opening").status.success());
    assert_eq!(
        fs::read_to_string(directory.0.join("opening/state.tsv")).unwrap(),
        state
    );
    assert_eq!(
        fs::read(directory.0.join("opening/000.cga")).unwrap(),
        first
    );
    assert!(!export("271", "unsupported").status.success());
    assert!(!directory.0.join("unsupported").exists());
    assert!(!export("0", "data2").status.success());
    assert_eq!(
        fs::read(directory.0.join("data2")).unwrap(),
        vec![0; 28_960]
    );
}
