// Copyright (c) 2026 analogued. Project license has not yet been selected.
use super::*;
#[test]
fn cache_roundtrips_geometry_rejects_corruption_and_replaces_stale_data() {
    let dir = std::env::temp_dir().join(format!("reforge-preview-cache-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("roundtrip.png");
    let colour = Frame::new(DisplayGeometry::new((2, 1), (16, 9)), vec![255; 8]).unwrap();
    store(&path, &colour).unwrap();
    assert!(!load(&path).unwrap().is_monochrome());
    let frame = Frame::new_monochrome(DisplayGeometry::new((2, 1), (16, 9)), vec![255; 8]).unwrap();
    store(&path, &frame).unwrap();
    let read = load(&path).unwrap();
    assert_eq!(read.geometry(), frame.geometry());
    assert_eq!(read.rgba(), frame.rgba());
    assert!(read.is_monochrome());
    fs::write(&path, b"broken cache").unwrap();
    assert!(load(&path).is_err());
    store(&path, &frame).unwrap();
    assert_eq!(load(&path).unwrap().rgba(), frame.rgba());
    fs::remove_file(path).unwrap();
    fs::remove_dir(dir).unwrap();
}
#[test]
fn cache_identity_is_release_content_and_visual_variant_based() {
    let release = &detection::RELEASES[0];
    assert_ne!(cache_id(release, "cga"), cache_id(release, "pcjr"));
    let different = Release {
        files: &[],
        ..*release
    };
    assert_ne!(cache_id(release, "cga"), cache_id(&different, "cga"));
    assert_eq!(cache_id(release, "cga").len(), 64);
}

struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "reforge-preview-recovery-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct TestProcess(std::process::Child);
impl Drop for TestProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn frame() -> Frame {
    Frame::new(
        DisplayGeometry::new((2, 1), (4, 3)),
        [128, 128, 128, 255].repeat(2),
    )
    .unwrap()
}
fn request(entry: &str) -> Request {
    Request {
        entry: entry.into(),
        release: "synthetic".into(),
        original: PathBuf::from("synthetic"),
        preferred: "cga".into(),
    }
}
fn sources(id: &str) -> BTreeMap<String, Source> {
    BTreeMap::from([(
        "cga".into(),
        Source {
            id: id.into(),
            frame: Arc::new(frame()),
        },
    )])
}
fn process(role: &str, root: &Path) -> TestProcess {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "launcher::preview::sources::tests::recovery_process_fixture",
            "--ignored",
            "--nocapture",
        ])
        .env("REFORGE_PREVIEW_TEST_ROLE", role)
        .env("REFORGE_PREVIEW_TEST_ROOT", root)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut child = TestProcess(child);
    let start = std::time::Instant::now();
    while !root.join("ready").exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "fixture process exited before ready"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "fixture process did not become ready"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    child
}
#[test]
#[ignore = "internal subprocess fixture; invoked with isolated environment by recovery tests"]
fn recovery_process_fixture() {
    let Some(role) = std::env::var_os("REFORGE_PREVIEW_TEST_ROLE") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("REFORGE_PREVIEW_TEST_ROOT").unwrap());
    let _cache = if role == "interrupted" {
        let cache = CacheSession::open(&root).unwrap();
        store(&root.join("first.png"), &frame()).unwrap();
        fs::write(
            root.join(format!(
                ".reforge-preview-second-{}-0.tmp",
                std::process::id()
            )),
            b"half-written PNG",
        )
        .unwrap();
        Some(cache)
    } else {
        None
    };
    fs::write(root.join("ready"), b"ready").unwrap();
    loop {
        std::thread::park_timeout(std::time::Duration::from_secs(1));
    }
}
#[test]
fn worker_completes_without_ui_polling_while_another_process_runs() {
    let directory = TestDirectory::new();
    let mut game = process("game", &directory.0);
    let (started, waiting) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let manager = Manager::with_generator(None, move |request, _| {
        started.send(()).unwrap();
        resume
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        sources(&request.entry)
    });
    manager.send.send(request("parallel")).unwrap();
    waiting
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(game.0.try_wait().unwrap().is_none());
    release.send(()).unwrap();
    // Do not call poll: the launcher likewise stops UI polling during gameplay.
    let reply = manager
        .receive
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(reply.frames["cga"].id, "parallel");
    assert!(game.0.try_wait().unwrap().is_none());
}
#[test]
fn generator_and_job_panics_do_not_stop_later_work() {
    assert!(cached_frame(None, || panic!("synthetic variant failure")).is_err());
    assert!(cached_frame(None, || Ok(frame())).is_ok());
    let manager = Manager::with_generator(None, |request, _| {
        if request.entry == "broken" {
            panic!("synthetic job failure");
        }
        sources(&request.entry)
    });
    manager.send.send(request("broken")).unwrap();
    manager.send.send(request("next")).unwrap();
    assert!(
        manager
            .receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .frames
            .is_empty()
    );
    assert_eq!(
        manager
            .receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .frames["cga"]
            .id,
        "next"
    );
}
#[test]
fn killed_generation_reuses_completed_variant_and_restarts_only_incomplete_work() {
    let directory = TestDirectory::new();
    let mut child = process("interrupted", &directory.0);
    let first = directory.0.join("first.png");
    let before = fs::read(&first).unwrap();
    fs::write(directory.0.join("unrelated.tmp"), b"keep").unwrap();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    // Lock ownership is recovered after process termination, not by guessing PIDs.
    let _cache = CacheSession::open(&directory.0).unwrap();
    assert!(
        fs::read_dir(&directory.0)
            .unwrap()
            .all(|e| !owned_temporary(&e.unwrap().file_name().to_string_lossy()))
    );
    assert!(directory.0.join("unrelated.tmp").exists());
    let completed = cached_frame(Some(&first), || {
        panic!("completed variant must not regenerate")
    })
    .unwrap();
    assert_eq!(completed.rgba(), frame().rgba());
    assert_eq!(fs::read(&first).unwrap(), before);
    let mut regenerated = 0;
    let second = directory.0.join("second.png");
    cached_frame(Some(&second), || {
        regenerated += 1;
        Ok(frame())
    })
    .unwrap();
    assert_eq!(regenerated, 1);
    assert_eq!(load(&second).unwrap().rgba(), frame().rgba());
    // A truncated checkpoint is rejected even if all pixel rows survived.
    let bytes = fs::read(&second).unwrap();
    fs::write(&second, &bytes[..bytes.len() - 12]).unwrap();
    assert!(load(&second).is_err());
    cached_frame(Some(&second), || {
        regenerated += 1;
        Ok(frame())
    })
    .unwrap();
    assert_eq!(regenerated, 2);
}
#[test]
fn failed_publication_preserves_destination_and_cleans_its_temporary() {
    let directory = TestDirectory::new();
    let target = directory.0.join("image.png");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), b"existing destination").unwrap();
    assert!(store(&target, &frame()).is_err());
    assert!(target.join("keep").exists());
    assert!(
        fs::read_dir(&directory.0)
            .unwrap()
            .all(|e| !owned_temporary(&e.unwrap().file_name().to_string_lossy()))
    );
}

#[test]
fn completed_work_is_not_queued_again_after_gameplay_refreshes_the_model() {
    let directory = TestDirectory::new();
    let release = &detection::RELEASES[0];
    let entry = reforge_settings::library::Entry {
        entry_id: "parallel".into(),
        game_id: release.game.id().into(),
        release_id: release.id.into(),
        title: release.metadata.title.into(),
        original: directory.0.join("original"),
        directory: directory.0.clone(),
    };
    let mut game = GameEntry::from_entry(&entry, directory.0.join("config.toml"), false);
    let mut manager =
        Manager::with_generator(None, |_, _| panic!("completed work was queued again"));
    let request = Request {
        entry: entry.entry_id,
        release: entry.release_id,
        original: entry.original,
        preferred: game.preview_variant().into(),
    };
    manager.pending.insert(request.clone());
    // A completed reply is buffered while the launcher is hidden. Returning
    // from the game rebuilds a model whose old in-flight flag is now false.
    let (send, receive) = mpsc::channel();
    manager.receive = receive;
    send.send(Reply {
        request,
        frames: sources("completed"),
    })
    .unwrap();
    game.preview_attempted = false;
    manager.poll(std::slice::from_mut(&mut game));
    assert!(game.preview_attempted);
    assert_eq!(game.previews["cga"].id, "completed");
    assert!(manager.pending.is_empty());
}
