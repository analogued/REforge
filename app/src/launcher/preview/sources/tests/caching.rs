// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn preview_request(root: &Path) -> Request {
    let release = &detection::RELEASES[0];
    Request {
        identity: Identity {
            entry: "cached-copy".into(),
            release: release.id.into(),
            original: root.join("original"),
            preferred: "cga".into(),
            allow_generation: true,
        },
        verification: None,
    }
}

fn detected(request: &Request) -> Detected {
    let release = &detection::RELEASES[0];
    Detected {
        game: release.game(),
        release,
        root: request.identity.original.parent().unwrap().to_owned(),
        original: request.identity.original.clone(),
        files: BTreeMap::from([(release.files[0].name, request.identity.original.clone())]),
    }
}

fn cached(root: &Path, variant: &str) -> PathBuf {
    root.join(format!(
        "{}.png",
        cache_id(&detection::RELEASES[0], variant)
    ))
}

fn populate(root: &Path) {
    for variant in alleycat::launcher::PREVIEW_VARIANTS {
        store(&cached(root, variant), &frame()).unwrap();
    }
}

#[test]
fn complete_cache_loads_without_detecting_or_rendering_even_when_originals_are_absent() {
    let directory = TestDirectory::new();
    let mut request = preview_request(&directory.0);
    populate(&directory.0);
    assert!(!request.identity.original.exists());
    for allow_generation in [true, false] {
        request.identity.allow_generation = allow_generation;
        let found = generate_with(
            &request,
            Some(&directory.0),
            |_| panic!("cached previews must not inspect originals"),
            |_, _| panic!("cached previews must not be regenerated"),
        );
        assert_eq!(found.len(), alleycat::launcher::PREVIEW_VARIANTS.len());
    }
}

#[test]
fn only_the_missing_or_corrupt_variant_is_regenerated() {
    for corrupt in [false, true] {
        let directory = TestDirectory::new();
        let request = preview_request(&directory.0);
        populate(&directory.0);
        let intact = cached(&directory.0, "cga");
        let before = fs::read(&intact).unwrap();
        let broken = cached(&directory.0, "pcjr");
        if corrupt {
            fs::write(&broken, b"broken PNG").unwrap();
        } else {
            fs::remove_file(&broken).unwrap();
        }
        let mut detections = 0;
        let mut rendered = Vec::new();
        let found = generate_with(
            &request,
            Some(&directory.0),
            |_| {
                detections += 1;
                Ok(detected(&request))
            },
            |_, variant| {
                rendered.push(variant.to_owned());
                Ok(frame())
            },
        );
        assert_eq!(detections, 1);
        assert_eq!(rendered, ["pcjr"]);
        assert_eq!(found.len(), 2);
        assert_eq!(fs::read(&intact).unwrap(), before);
        assert_eq!(load(&broken).unwrap().rgba(), frame().rgba());
        // The repaired cache is used on the next launcher session without
        // requiring the originals to still be available.
        assert_eq!(
            generate_with(
                &request,
                Some(&directory.0),
                |_| panic!("unneeded detection"),
                |_, _| panic!("unneeded generation")
            )
            .len(),
            2
        );
    }
}

#[test]
fn multiple_missing_variants_share_one_full_verification() {
    let directory = TestDirectory::new();
    let request = preview_request(&directory.0);
    let mut detections = 0;
    let mut renderings = 0;
    let found = generate_with(
        &request,
        Some(&directory.0),
        |_| {
            detections += 1;
            Ok(detected(&request))
        },
        |_, _| {
            renderings += 1;
            Ok(frame())
        },
    );
    assert_eq!(found.len(), 2);
    assert_eq!(detections, 1);
    assert_eq!(renderings, 2);
}

#[test]
fn fresh_library_verification_is_reused_for_initial_generation() {
    let directory = TestDirectory::new();
    let mut request = preview_request(&directory.0);
    // Authored bytes cannot pass the real catalog hash. Only the supplied,
    // already-completed verification should be used for this job.
    File::create(&request.identity.original)
        .unwrap()
        .set_len(detection::RELEASES[0].files[0].size)
        .unwrap();
    request.verification = Some(Arc::new(
        Verification::capture(&request.identity.original, detected(&request)).unwrap(),
    ));
    let found = generate_with(
        &request,
        Some(&directory.0),
        |_| panic!("duplicate original-file hashing"),
        |originals, _| {
            assert_eq!(originals.files, detected(&request).files);
            Ok(frame())
        },
    );
    assert_eq!(found.len(), 2);
}

#[test]
fn changed_sources_are_reverified_once_and_valid_cached_variants_survive_failure() {
    let directory = TestDirectory::new();
    let mut request = preview_request(&directory.0);
    let original = File::create(&request.identity.original).unwrap();
    original
        .set_len(detection::RELEASES[0].files[0].size)
        .unwrap();
    request.verification = Some(Arc::new(
        Verification::capture(&request.identity.original, detected(&request)).unwrap(),
    ));
    let modified =
        original.metadata().unwrap().modified().unwrap() + std::time::Duration::from_secs(5);
    original
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    store(&cached(&directory.0, "pcjr"), &frame()).unwrap();
    let mut detections = 0;
    let found = generate_with(
        &request,
        Some(&directory.0),
        |_| {
            detections += 1;
            Err("changed sources are no longer supported".into())
        },
        |_, _| panic!("unverified sources must not be rendered"),
    );
    assert_eq!(detections, 1);
    assert_eq!(found.len(), 1);
    assert!(found.contains_key("pcjr"));
    assert!(!cached(&directory.0, "cga").exists());
}

#[test]
fn another_release_cannot_generate_under_the_requested_cache_identity() {
    let directory = TestDirectory::new();
    let request = preview_request(&directory.0);
    let release = detection::RELEASES
        .iter()
        .find(|release| release.id != request.identity.release)
        .unwrap();
    let mut detections = 0;
    let found = generate_with(
        &request,
        Some(&directory.0),
        |_| {
            detections += 1;
            Ok(Detected {
                game: release.game(),
                release,
                ..detected(&request)
            })
        },
        |_, _| panic!("wrong release must not be rendered"),
    );
    assert_eq!(detections, 1);
    assert!(found.is_empty());
    assert!(!cached(&directory.0, "cga").exists());
    assert!(!cached(&directory.0, "pcjr").exists());
}

#[test]
fn missing_originals_load_partial_cache_through_the_manager() {
    let directory = TestDirectory::new();
    let request = preview_request(&directory.0);
    store(&cached(&directory.0, "cga"), &frame()).unwrap();
    fs::write(cached(&directory.0, "pcjr"), b"broken PNG").unwrap();
    let release = &detection::RELEASES[0];
    let entry = reforge_settings::library::Entry {
        entry_id: request.identity.entry,
        game_id: release.game().id().into(),
        release_id: release.id.into(),
        title: release.edition.title().into(),
        original: request.identity.original,
        directory: directory.0.clone(),
    };
    let mut game = GameEntry::from_entry(&entry, directory.0.join("config.toml"), false);
    game.status = EntryStatus::Missing("unavailable".into());
    let mut manager = Manager::with_generator(Some(directory.0.clone()), |request, cache| {
        assert!(!request.identity.allow_generation);
        generate_with(
            request,
            cache,
            |_| panic!("missing originals must not be reverified"),
            |_, _| panic!("missing originals must not be rendered"),
        )
    });
    manager.poll(std::slice::from_mut(&mut game));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while game.previews.is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "cached preview was not delivered"
        );
        manager.poll(std::slice::from_mut(&mut game));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(game.previews.len(), 1);
    assert!(game.previews.contains_key("cga"));
    assert!(game.status.is_missing());
}

#[test]
fn partial_worker_replies_preserve_valid_images_already_in_memory() {
    let directory = TestDirectory::new();
    let mut request = preview_request(&directory.0);
    request.identity.allow_generation = false;
    let release = &detection::RELEASES[0];
    let entry = reforge_settings::library::Entry {
        entry_id: request.identity.entry.clone(),
        game_id: release.game().id().into(),
        release_id: release.id.into(),
        title: release.edition.title().into(),
        original: request.identity.original.clone(),
        directory: directory.0.clone(),
    };
    let mut game = GameEntry::from_entry(&entry, directory.0.join("config.toml"), false);
    game.status = EntryStatus::Missing("unavailable".into());
    game.preview_attempted = true;
    game.previews = sources("valid-in-memory");
    let mut manager = Manager::with_generator(None, |_, _| panic!("unneeded cache job"));
    let (send, receive) = mpsc::channel();
    manager.receive = receive;
    send.send(Reply {
        request,
        frames: BTreeMap::new(),
    })
    .unwrap();
    manager.poll(std::slice::from_mut(&mut game));
    assert_eq!(game.previews["cga"].id, "valid-in-memory");
    assert!(game.preview_attempted);
}
