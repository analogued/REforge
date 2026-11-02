// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "frontend")]

use btz::{
    player::files::Files,
    services::{HostReply, HostRequest},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "btz-player-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn files(&self) -> Files {
        Files::new(&self.0.join("ADISK.OBJ")).unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn windowed_filename_submission_uses_the_same_save_directory_and_bytes() {
    let directory = Directory::new();
    let mut files = directory.files();
    let save = HostRequest::SaveBookmark {
        data: b"authored native-state bytes".to_vec(),
    };
    assert!(files.prompt(&save).is_some());
    assert!(matches!(
        files
            .handle_with_path(&save, Some("checkpoint".into()))
            .unwrap(),
        Some(HostReply::Done)
    ));
    assert_eq!(
        fs::read(directory.0.join("checkpoint")).unwrap(),
        b"authored native-state bytes"
    );
    assert!(
        matches!(files.handle_with_path(&HostRequest::LoadBookmark, Some("checkpoint".into())).unwrap(), Some(HostReply::Bytes(bytes)) if bytes == b"authored native-state bytes")
    );
    assert!(files.take_error().is_none());
}

#[test]
fn cancellation_and_fixed_paths_need_no_console_input() {
    let directory = Directory::new();
    let mut files = directory.files();
    let save = HostRequest::SaveBookmark { data: vec![1, 2] };
    for request in [&save, &HostRequest::LoadBookmark] {
        assert!(matches!(
            files.handle_with_path(request, None).unwrap(),
            Some(HostReply::Cancelled)
        ));
    }
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 0);
    files.bookmark_path = Some(directory.0.join("fixed"));
    assert!(files.prompt(&save).is_none());
    assert!(matches!(
        files.handle_with_path(&save, None).unwrap(),
        Some(HostReply::Done)
    ));
    assert_eq!(fs::read(directory.0.join("fixed")).unwrap(), [1, 2]);
}

#[test]
fn file_errors_are_available_for_windowed_feedback_and_originals_are_protected() {
    let directory = Directory::new();
    let mut files = directory.files();
    let original = directory.0.join("ADISK.OBJ");
    fs::write(&original, b"original").unwrap();
    files.originals.push(original.clone());
    let save = HostRequest::SaveBookmark { data: vec![0] };
    assert!(matches!(
        files
            .handle_with_path(&save, Some(original.clone()))
            .unwrap(),
        Some(HostReply::Cancelled)
    ));
    assert_eq!(fs::read(original).unwrap(), b"original");
    assert!(files.take_error().unwrap().contains("original game input"));
    assert!(files.take_error().is_none());
    assert!(matches!(
        files
            .handle_with_path(&HostRequest::LoadBookmark, Some("missing".into()))
            .unwrap(),
        Some(HostReply::Cancelled)
    ));
    assert!(files.take_error().unwrap().contains("Cannot read bookmark"));
}

#[test]
fn loaded_transcript_paths_are_host_paths_and_preserve_game_directory_default() {
    let directory = Directory::new();
    let mut files = directory.files();
    let request = HostRequest::TranscriptName {
        prompt: Vec::new(),
        max_bytes: 2,
    };
    let reply = files
        .handle_with_path(&request, Some("session.txt".into()))
        .unwrap();
    let Some(HostReply::Name(name)) = reply else {
        panic!("expected transcript name")
    };
    files
        .handle_with_path(&HostRequest::OpenTranscript { path: name }, None)
        .unwrap();
    files
        .handle_with_path(
            &HostRequest::WriteTranscript {
                bytes: b"game and command echoes\r\n".to_vec(),
            },
            None,
        )
        .unwrap();
    files
        .handle_with_path(&HostRequest::CloseTranscript, None)
        .unwrap();
    assert_eq!(
        fs::read(directory.0.join("session.txt")).unwrap(),
        b"game and command echoes\r\n"
    );
}

#[test]
fn a_transcript_hardlink_alias_does_not_truncate_original_data() {
    let directory = Directory::new();
    let mut files = directory.files();
    let original = directory.0.join("ADISK.OBJ");
    let alias = directory.0.join("session.txt");
    fs::write(&original, b"original input").unwrap();
    fs::hard_link(&original, &alias).unwrap();
    files.originals.push(original.clone());
    files.transcript_path = Some(alias.clone());
    files
        .handle_with_path(&HostRequest::OpenTranscript { path: vec![] }, None)
        .unwrap();
    files
        .handle_with_path(
            &HostRequest::WriteTranscript {
                bytes: b"session output".to_vec(),
            },
            None,
        )
        .unwrap();
    files
        .handle_with_path(&HostRequest::CloseTranscript, None)
        .unwrap();
    assert_eq!(fs::read(original).unwrap(), b"original input");
    assert_eq!(fs::read(alias).unwrap(), b"session output");
}
