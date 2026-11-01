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

fn program_file() -> Vec<u8> {
    let mut image = vec![0; 0xa500];
    image[mach3::executable::DATA_SEGMENT_OFFSET + 0x04ac
        ..mach3::executable::DATA_SEGMENT_OFFSET + 0x04af]
        .copy_from_slice(&[1, 2, 3]);
    for index in 0..21 {
        image[mach3::executable::DATA_SEGMENT_OFFSET + 0x0a3e + 50 * index] = 0xcf;
    }
    pack_program(image)
}

fn pack_program(mut image: Vec<u8>) -> Vec<u8> {
    let size = image.len() as u16;
    image.extend(size.to_le_bytes());
    image.push(0xb3);
    let stub_offset = image.len().div_ceil(16) * 16;
    image.resize(stub_offset, 255);
    image.extend([0; 16]);
    image[stub_offset + 12..stub_offset + 14].copy_from_slice(&(size / 16).to_le_bytes());
    image[stub_offset + 14..stub_offset + 16].copy_from_slice(&0x4252_u16.to_le_bytes());
    let mut header = vec![0; 32];
    for (offset, value) in [
        (0, 0x5a4d_u16),
        (8, 2),
        (20, 16),
        (22, (stub_offset / 16) as u16),
    ] {
        header[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    header.extend(image);
    header
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
    fs::write(directory.0.join("mach3.exe"), program_file()).unwrap();
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
    assert!(
        !export(
            &(mach3::gameplay::DEFAULT_EXPORT_BODY_LIMIT + 1).to_string(),
            "unsupported"
        )
        .status
        .success()
    );
    assert!(!directory.0.join("unsupported").exists());
    assert!(!export("0", "data2").status.success());
    assert_eq!(
        fs::read(directory.0.join("data2")).unwrap(),
        vec![0; 28_960]
    );
}

#[test]
fn short_expanded_program_reports_an_error_instead_of_panicking() {
    let directory = TestDirectory::new();
    for (name, bytes) in [
        ("DATA1", 0x4000),
        ("DATA2", 28_960),
        ("DATA3", 16_000),
        ("DATA6", 0x4000),
    ] {
        fs::write(directory.0.join(name), vec![0; bytes]).unwrap();
    }
    fs::write(directory.0.join("DATA4"), sprite_file(0)).unwrap();
    fs::write(directory.0.join("DATA5"), sprite_file(255)).unwrap();
    fs::write(directory.0.join("MACH3.EXE"), pack_program(vec![0; 64])).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_mach3-opening"))
        .args(["--steps", "0"])
        .arg(&directory.0)
        .arg(directory.0.join("output"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert!(String::from_utf8_lossy(&result.stderr).contains("truncated program data segment"));
    assert!(!directory.0.join("output").exists());
    #[cfg(feature = "sdl")]
    {
        let result = Command::new(env!("CARGO_BIN_EXE_mach-3"))
            .args(["--static", "--original"])
            .arg(&directory.0)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1), "{result:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("truncated program data segment"));
    }
}
