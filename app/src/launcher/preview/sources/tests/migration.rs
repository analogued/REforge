// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn all_legacy_images_migrate_without_originals_and_are_not_regenerated() {
    let directory = TestDirectory::new();
    let mut covered = 0;
    for release in detection::RELEASES {
        let variants = match release.game() {
            Game::AlleyCat => alleycat::launcher::PREVIEW_VARIANTS,
            Game::Mach3 => mach3::launcher::PREVIEW_VARIANTS,
            _ => btz::launcher::PREVIEW_VARIANTS,
        };
        // New distributions need not have any pre-BLAKE3 preview aliases.
        let variants: Vec<_> = variants
            .iter()
            .copied()
            .filter(|variant| {
                let current = cache_id(release, variant);
                detection::migration::PREVIEWS
                    .iter()
                    .any(|(key, _)| *key == current)
            })
            .collect();
        if variants.is_empty() {
            continue;
        }
        for variant in &variants {
            let current = cache_id(release, variant);
            for (_, old) in detection::migration::PREVIEWS
                .iter()
                .filter(|(key, _)| *key == current)
            {
                store(&directory.0.join(format!("{old}.png")), &frame()).unwrap();
                covered += 1;
            }
        }
        let request = Request {
            identity: Identity {
                entry: "missing-originals".into(),
                game: release.game().id().into(),
                directory: directory.0.join("unavailable"),
                original: directory.0.join("unavailable"),
                release: release.id.into(),
                preferred: variants[0].into(),
                allow_generation: false,
                generation: 0,
            },
            verification: None,
        };
        for _ in 0..2 {
            let result = generate_with(
                &request,
                Some(&directory.0),
                |_| panic!("unnecessary verification"),
                |_, _| panic!("unnecessary regeneration"),
            );
            assert_eq!(result.len(), variants.len());
        }
        for variant in &variants {
            let current = cache_id(release, variant);
            assert!(load(&directory.0.join(format!("{current}.png"))).is_ok());
            for (_, old) in detection::migration::PREVIEWS
                .iter()
                .filter(|(key, _)| *key == current)
            {
                assert!(!directory.0.join(format!("{old}.png")).exists());
            }
        }
    }
    assert_eq!(covered, detection::migration::PREVIEWS.len());
}

#[test]
fn each_published_or_development_alias_can_migrate_by_itself() {
    for (new, old) in detection::migration::PREVIEWS {
        let directory = TestDirectory::new();
        let (release, variant) = detection::RELEASES
            .iter()
            .find_map(|release| {
                let variants = match release.game() {
                    Game::AlleyCat => alleycat::launcher::PREVIEW_VARIANTS,
                    Game::Mach3 => mach3::launcher::PREVIEW_VARIANTS,
                    _ => btz::launcher::PREVIEW_VARIANTS,
                };
                variants
                    .iter()
                    .find(|variant| cache_id(release, variant) == *new)
                    .map(|variant| (release, *variant))
            })
            .unwrap();
        store(&directory.0.join(format!("{old}.png")), &frame()).unwrap();
        let request = Request {
            identity: Identity {
                entry: "unavailable".into(),
                game: release.game().id().into(),
                directory: directory.0.clone(),
                original: directory.0.join("absent"),
                release: release.id.into(),
                preferred: variant.into(),
                allow_generation: false,
                generation: 0,
            },
            verification: None,
        };
        let frames = generate_with(
            &request,
            Some(&directory.0),
            |_| panic!("verification"),
            |_, _| panic!("regeneration"),
        );
        assert_eq!(frames[variant].id, *new);
        assert!(!directory.0.join(format!("{old}.png")).exists());
        assert!(load(&directory.0.join(format!("{new}.png"))).is_ok());
    }
}

#[test]
fn current_cache_wins_and_failed_migration_keeps_the_legacy_image_usable() {
    let directory = TestDirectory::new();
    let old = directory.0.join("old.png");
    let new = directory.0.join("new.png");
    store(&old, &frame()).unwrap();
    // A directory at the new destination forces rename failure without privileges.
    fs::create_dir(&new).unwrap();
    assert!(
        cached_frame(Some(&new), std::slice::from_ref(&old), || panic!(
            "regeneration"
        ))
        .is_ok()
    );
    assert!(load(&old).is_ok());
    fs::remove_dir(&new).unwrap();
    let current =
        Frame::new_monochrome(DisplayGeometry::new((2, 1), (4, 3)), vec![255; 8]).unwrap();
    store(&new, &current).unwrap();
    assert!(
        cached_frame(Some(&new), std::slice::from_ref(&old), || panic!(
            "regeneration"
        ))
        .unwrap()
        .is_monochrome()
    );
    assert!(!old.exists());
    store(&old, &frame()).unwrap();
    fs::write(&new, b"corrupt").unwrap();
    assert!(
        !cached_frame(Some(&new), std::slice::from_ref(&old), || panic!(
            "regeneration"
        ))
        .unwrap()
        .is_monochrome()
    );
    assert!(!old.exists());
}

#[test]
fn corrupt_legacy_images_are_not_migrated_and_manifest_changes_have_no_alias() {
    let directory = TestDirectory::new();
    let old = directory.0.join("old.png");
    let new = directory.0.join("new.png");
    fs::write(&old, b"corrupt").unwrap();
    assert!(
        cached_frame(Some(&new), std::slice::from_ref(&old), || Err(
            "originals unavailable".into()
        ))
        .is_err()
    );
    assert!(!new.exists());
    let release = &detection::RELEASES[0];
    let files = [detection::FileRecord {
        blake3: "changed manifest",
        ..release.files[0]
    }];
    let files = Box::leak(Box::new(files));
    let changed = Release { files, ..*release };
    let key = cache_id(&changed, "cga");
    assert!(
        !detection::migration::PREVIEWS
            .iter()
            .any(|(new, _)| *new == key)
    );
}
