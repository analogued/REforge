// Copyright (c) 2026 analogued.
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::launcher::model::State;
use reforge_settings::library::{Entry, Library};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(5);

struct ControlledWorker {
    manager: Manager,
    started: mpsc::Receiver<Request>,
    finish: mpsc::Sender<BTreeMap<String, Source>>,
}
impl ControlledWorker {
    fn new() -> Self {
        let (started, receive) = mpsc::channel();
        let (finish, resume) = mpsc::channel();
        let manager = Manager::with_generator(None, move |request, _| {
            started.send(request.clone()).unwrap();
            resume.recv_timeout(TIMEOUT).unwrap()
        });
        Self {
            manager,
            started: receive,
            finish,
        }
    }
    fn next(&self) -> Request {
        self.started.recv_timeout(TIMEOUT).expect("job not started")
    }
    fn complete(&mut self, state: &mut State) {
        let deadline = Instant::now() + TIMEOUT;
        while !state.games[0].previews_complete() || !self.manager.pending.is_empty() {
            self.manager.poll(&mut state.games);
            assert!(Instant::now() < deadline, "repair was not delivered");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(state.games[0].previews["cga"].id, "old-a");
        assert_eq!(state.games[0].previews["pcjr"].id, "repaired-a");
        for _ in 0..5 {
            self.manager.poll(&mut state.games);
        }
        assert!(self.started.try_recv().is_err(), "redundant preview job");
        assert!(self.manager.pending.is_empty());
    }
}

// Supply verification results to isolate actual persistence, reconciliation and
// worker scheduling. The files contain authored bytes, not supported game data.
fn detected(root: &Path) -> Detected {
    let release = &detection::RELEASES[0];
    let original = root.join("authored.bin");
    Detected {
        game: release.game(),
        release,
        root: root.to_owned(),
        original: original.clone(),
        files: BTreeMap::from([(release.files[0].name, original)]),
    }
}
fn verified(entry: &Entry) -> Result<Detected, String> {
    Ok(detected(&entry.directory))
}
fn setup(directory: &TestDirectory) -> (State, PathBuf, PathBuf) {
    let a = directory.0.join("a");
    let b = directory.0.join("b");
    for root in [&a, &b] {
        fs::create_dir(root).unwrap();
        fs::write(root.join("authored.bin"), b"authored fixture").unwrap();
    }
    let path = directory.0.join("library.toml");
    let config = directory.0.join("config.toml");
    let mut state = State::new_with(path.clone(), config.clone(), verified);
    state.library = Library::new(path).with_storage(config, None);
    state
        .add_with(&a, |root| Ok(detected(root)), verified)
        .unwrap();
    (state, a, b)
}
fn repaired() -> BTreeMap<String, Source> {
    BTreeMap::from([(
        "pcjr".into(),
        Source {
            id: "repaired-a".into(),
            frame: Arc::new(frame()),
        },
    )])
}

#[test]
fn same_model_recovery_delivers_the_repair_despite_a_late_reply() {
    for allow_generation in [false, true] {
        let directory = TestDirectory::new();
        let (mut state, _, _) = setup(&directory);
        if !allow_generation {
            state.games[0].status = EntryStatus::Missing("unavailable".into());
        }
        let mut worker = ControlledWorker::new();
        worker.manager.poll(&mut state.games);
        let old = worker.next();
        assert_eq!(old.identity.allow_generation, allow_generation);
        let entry = state.library.entries[0].clone();
        state.apply_verification(&entry, &verified(&entry));
        worker.manager.poll(&mut state.games);
        worker.finish.send(sources("old-a")).unwrap();
        let fresh = worker.next(); // The old reply is now on the real reply channel.
        assert_ne!(fresh.identity.generation, old.identity.generation);
        assert!(fresh.identity.allow_generation);
        worker.finish.send(repaired()).unwrap();
        worker.complete(&mut state);
    }
}

#[test]
fn replacement_and_return_to_the_same_identity_delivers_the_repair() {
    for external_reinsert in [false, true] {
        for reply_before_enqueue in [false, true] {
            let directory = TestDirectory::new();
            let (mut state, a, b) = setup(&directory);
            let id = state.selected.clone().unwrap();
            let path = directory.0.join("library.toml");
            let saved_a = fs::read_to_string(&path).unwrap();
            let mut worker = ControlledWorker::new();
            worker.manager.poll(&mut state.games);
            let old = worker.next(); // Keep the first A job blocked through replacement.
            state
                .relocate_with(id.clone(), &b, |root| Ok(detected(root)), verified)
                .unwrap();
            worker.manager.poll(&mut state.games); // B waits behind the old A job.
            if external_reinsert {
                fs::write(&path, "schema_version = 2\n").unwrap();
                state.refresh_with(verified);
                assert!(state.games.is_empty());
                fs::write(&path, &saved_a).unwrap();
                state.refresh_with(verified);
            } else {
                state
                    .relocate_with(id.clone(), &a, |root| Ok(detected(root)), verified)
                    .unwrap();
            }
            assert_eq!(state.games[0].entry_id, id);
            assert_eq!(state.games[0].directory, a);
            if !reply_before_enqueue {
                worker.manager.poll(&mut state.games);
            }
            worker.finish.send(sources("old-a")).unwrap();
            // B starting proves the worker sent A's old reply before this poll.
            assert_eq!(worker.next().identity.directory, b);
            worker.manager.poll(&mut state.games);
            assert_eq!(state.games[0].previews["cga"].id, "old-a");
            worker.finish.send(sources("obsolete-b")).unwrap();
            let fresh = worker.next();
            assert_eq!(fresh.identity.directory, a);
            assert_ne!(fresh.identity.generation, old.identity.generation);
            worker.finish.send(repaired()).unwrap();
            worker.complete(&mut state);
        }
    }
}
