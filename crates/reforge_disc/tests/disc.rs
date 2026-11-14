// Copyright (c) 2026 analogued. SPDX-License-Identifier: GPL-3.0-or-later
//! Independent storage/index/bounds checks using authored ISO metadata only.
mod support;

use reforge_disc::{Disc, Limits, Region};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
};
use support::{BLOCK, Tree, dual16, dual32, iso, record};

fn files() -> Vec<(String, Vec<u8>)> {
    vec![
        ("Assets/Intro.bin".into(), b"nested payload".to_vec()),
        ("Assets/Sub/End.bin".into(), b"deeper payload".to_vec()),
        ("README".into(), b"root payload".to_vec()),
        ("EMPTY".into(), Vec::new()),
    ]
}

fn read(disc: &Disc, name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    disc.get(name)
        .unwrap()
        .open()
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    out
}

#[test]
fn directory_and_content_detected_iso_have_the_same_hierarchical_files() {
    let tree = Tree::new();
    let files = files();
    let directory = tree.directory(&files);
    let image = tree.write("not-an-iso-extension.data", &iso(&files));
    let a = Disc::open(&directory).unwrap();
    let b = Disc::open(&image).unwrap();
    assert_eq!(a.source(), fs::canonicalize(directory).unwrap());
    assert_eq!(b.source(), fs::canonicalize(image).unwrap());
    let names = ["ASSETS/INTRO.BIN", "ASSETS/SUB/END.BIN", "EMPTY", "README"];
    for disc in [&a, &b] {
        assert_eq!(
            disc.files().keys().map(String::as_str).collect::<Vec<_>>(),
            names
        );
        for (name, data) in &files {
            assert_eq!(read(disc, name), *data);
            assert_eq!(
                disc.get(&name.to_ascii_lowercase()).unwrap().len(),
                data.len() as u64
            );
        }
        assert!(disc.get("EMPTY").unwrap().is_empty());
        assert_eq!(
            disc.get("MISSING").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        for invalid in [
            "",
            "/README",
            "../README",
            "ASSETS/./INTRO.BIN",
            "ASSETS//INTRO.BIN",
            "README;1",
            "ASSETS\\INTRO.BIN",
            "README/",
        ] {
            assert_eq!(
                disc.get(invalid).unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "{invalid:?}"
            );
        }
    }
    // Streams own their backing handle even after the container index is consumed.
    let index = b.into_files();
    assert_eq!(index["ASSETS/INTRO.BIN"].read_at(0, 6).unwrap(), b"nested");
}

#[test]
fn iso_version_one_suffix_and_empty_extension_are_storage_details() {
    let tree = Tree::new();
    let image = tree.write("disc.iso", &iso(&[("README.".into(), b"text".to_vec())]));
    let disc = Disc::open(image).unwrap();
    assert_eq!(read(&disc, "readme"), b"text");
    assert_eq!(disc.files().len(), 1);
}

#[test]
fn bounded_streams_have_independent_positions_and_do_not_expose_neighbors() {
    let tree = Tree::new();
    let path = tree.write("bytes", b"0123456789abcdef");
    let file = Region::from_file(File::open(path).unwrap()).unwrap();
    let region = file.slice(4, 8).unwrap();
    assert_eq!(region.len(), 8);
    let mut a = region.open().unwrap();
    let mut b = region.clone().open().unwrap();
    let mut two = [0; 2];
    a.read_exact(&mut two).unwrap();
    assert_eq!(&two, b"45");
    b.read_exact(&mut two).unwrap();
    assert_eq!(&two, b"45");
    let mut three = [0; 3];
    a.read_exact(&mut three).unwrap();
    assert_eq!(&three, b"678");
    assert_eq!(b.seek(SeekFrom::End(-1)).unwrap(), 7);
    let mut tail = Vec::new();
    b.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, b"b");
    a.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, b"b9ab");
    assert_eq!(a.seek(SeekFrom::Start(100)).unwrap(), 100);
    assert_eq!(a.read(&mut two).unwrap(), 0);
    assert_eq!(
        a.seek(SeekFrom::Current(-101)).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(a.stream_position().unwrap(), 100);
    a.seek(SeekFrom::Start(u64::MAX)).unwrap();
    assert_eq!(a.read(&mut two).unwrap(), 0);
    a.seek(SeekFrom::End(-8)).unwrap();
    assert_eq!(a.read(&mut []).unwrap(), 0);
    assert_eq!(region.read_at(2, 4).unwrap(), b"6789");
    assert!(region.slice(8, 0).unwrap().is_empty());
    for (offset, len) in [(9, 0), (u64::MAX, 0), (1, u64::MAX)] {
        assert_eq!(
            region.slice(offset, len).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    // The range check must happen before allocating a caller-supplied length.
    assert_eq!(
        region.read_at(0, usize::MAX).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        region.read_at(u64::MAX, 1).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn truncated_backing_files_report_errors_instead_of_short_successful_payloads() {
    let tree = Tree::new();
    let path = tree.write("pinned", b"abcdefgh");
    let region = Region::from_file(File::open(&path).unwrap()).unwrap();
    fs::write(&path, b"ab").unwrap();
    assert_eq!(
        region.read_at(0, 8).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );

    let directory = tree.directory(&[("DATA.BIN".into(), b"abcdefgh".to_vec())]);
    let disc = Disc::open(&directory).unwrap();
    fs::write(directory.join("DATA.BIN"), b"ab").unwrap();
    assert_eq!(
        disc.get("DATA.BIN").unwrap().open().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn large_payloads_are_indexed_and_seeked_with_small_reads() {
    let tree = Tree::new();
    let path = tree.path("large.iso");
    let payload_len = 400 * 1024 * 1024_u64 + 123;
    let extent = 20_u64;
    let image_len = (extent * BLOCK as u64 + payload_len).div_ceil(BLOCK as u64) * BLOCK as u64;
    let mut metadata = iso(&[("LARGE.DAT".into(), Vec::new())]);
    for sector in [16, 17] {
        dual32(
            &mut metadata[sector * BLOCK + 80..sector * BLOCK + 88],
            (image_len / BLOCK as u64) as u32,
        );
    }
    dual32(
        &mut metadata[19 * BLOCK + 68 + 10..19 * BLOCK + 68 + 18],
        payload_len as u32,
    );
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(&metadata).unwrap();
    file.set_len(image_len).unwrap(); // Sparse fixture; never allocate its payload in memory.
    file.seek(SeekFrom::Start(extent * BLOCK as u64)).unwrap();
    file.write_all(b"HEAD").unwrap();
    file.seek(SeekFrom::Start(extent * BLOCK as u64 + payload_len - 4))
        .unwrap();
    file.write_all(b"TAIL").unwrap();
    drop(file);

    let disc = Disc::with_limits(
        &path,
        Limits {
            max_index_bytes: BLOCK,
            ..Limits::default()
        },
    )
    .unwrap();
    let data = disc.get("large.dat").unwrap();
    assert_eq!(data.len(), payload_len);
    assert_eq!(data.read_at(0, 4).unwrap(), b"HEAD");
    assert_eq!(data.read_at(1024 * 1024, 4).unwrap(), [0; 4]);
    let mut stream = data.open().unwrap();
    stream.seek(SeekFrom::End(-4)).unwrap();
    let mut tail = Vec::new();
    stream.read_to_end(&mut tail).unwrap();
    assert_eq!(tail, b"TAIL");
}

#[test]
fn iso_rejects_invalid_endian_extents_roots_records_and_descriptor_sets() {
    let tree = Tree::new();
    let original = iso(&[("DATA.BIN".into(), b"data".to_vec())]);
    for case in 0..10 {
        let mut bytes = original.clone();
        let pvd = 16 * BLOCK;
        let entry = 19 * BLOCK + 68;
        match case {
            0 => bytes[pvd + 80] ^= 1,
            1 => bytes.truncate(bytes.len() - BLOCK),
            2 => dual32(&mut bytes[entry + 2..entry + 10], 999),
            3 => bytes[pvd + 156] = 33,
            4 => bytes[entry] = 12,
            5 => bytes[18 * BLOCK + 1] = b'X',
            6 => bytes[17 * BLOCK + 40] ^= 1,
            7 => bytes[entry + 28] = 2, // Contradictory endian volume-sequence value.
            8 => {
                for sector in [16, 17] {
                    dual16(&mut bytes[sector * BLOCK + 128..sector * BLOCK + 132], 512);
                }
            }
            9 => {
                bytes[19 * BLOCK..20 * BLOCK].fill(0);
                for i in 0..60 {
                    let start = 19 * BLOCK + i * 34;
                    bytes[start..start + 34].copy_from_slice(&record(&[0], 19, BLOCK as u32, true));
                }
                bytes[19 * BLOCK + 2040] = 34; // The next record crosses its sector.
            }
            _ => unreachable!(),
        }
        let path = tree.write(&format!("bad-{case}.iso"), &bytes);
        assert!(Disc::open(path).is_err(), "malformed case {case}");
    }
}

#[test]
fn iso_rejects_cycles_duplicate_paths_and_unsupported_file_layouts() {
    let tree = Tree::new();
    let mut cycle = iso(&[("DIR/FILE".into(), b"data".to_vec())]);
    dual32(&mut cycle[19 * BLOCK + 68 + 2..19 * BLOCK + 68 + 10], 19);
    let error = Disc::open(tree.write("cycle.iso", &cycle)).unwrap_err();
    assert!(error.to_string().contains("cycle"));
    let duplicate = iso(&[
        ("name".into(), b"one".to_vec()),
        ("NAME".into(), b"two".to_vec()),
    ]);
    let error = Disc::open(tree.write("duplicate.iso", &duplicate)).unwrap_err();
    assert!(error.to_string().contains("ambiguous"));

    let original = iso(&[("DATA.BIN".into(), b"data".to_vec())]);
    for (case, offset, value) in [
        ("multi-extent", 25, 0x80),
        ("extended-attributes", 1, 1),
        ("interleaved", 26, 1),
    ] {
        let mut bytes = original.clone();
        bytes[19 * BLOCK + 68 + offset] |= value;
        assert_eq!(
            Disc::open(tree.write(&format!("{case}.iso"), &bytes))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Unsupported
        );
    }
    let mut version = original;
    let name = 19 * BLOCK + 68 + 33;
    version[name + "DATA.BIN;".len()] = b'2';
    assert_eq!(
        Disc::open(tree.write("version.iso", &version))
            .unwrap_err()
            .kind(),
        io::ErrorKind::Unsupported
    );
}

#[test]
fn indexing_limits_bound_metadata_and_entries_without_capping_file_payloads() {
    let tree = Tree::new();
    let files = files();
    let directory = tree.directory(&files);
    let image = tree.write("disc.iso", &iso(&files));
    for path in [&directory, &image] {
        for limits in [
            Limits {
                max_entries: 1,
                ..Limits::default()
            },
            Limits {
                max_index_bytes: 1,
                ..Limits::default()
            },
            Limits {
                max_depth: 1,
                ..Limits::default()
            },
        ] {
            assert!(Disc::with_limits(path, limits).is_err());
        }
    }
}

#[cfg(unix)]
#[test]
fn directory_indices_reject_symlinks_and_case_ambiguous_entries() {
    let tree = Tree::new();
    let directory = tree.directory(&[("DATA.BIN".into(), b"data".to_vec())]);
    assert_eq!(
        Region::from_file(File::open(&directory).unwrap())
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    std::os::unix::fs::symlink("DATA.BIN", directory.join("LINK.BIN")).unwrap();
    assert_eq!(
        Disc::open(&directory).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    fs::remove_file(directory.join("LINK.BIN")).unwrap();
    fs::write(directory.join("data.bin"), b"ambiguous").unwrap();
    // Some Unix filesystems fold case and cannot represent this ambiguity.
    if fs::read_dir(&directory).unwrap().count() == 2 {
        assert_eq!(
            Disc::open(directory).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
