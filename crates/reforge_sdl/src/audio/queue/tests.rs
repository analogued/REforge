// Copyright (c) 2026 analogued. Project license has not been selected.
use super::*;
#[derive(Default)]
struct Device {
    input: VecDeque<i16>,
    converted: VecDeque<i16>,
    played: Vec<i16>,
    paused: bool,
    clears: usize,
}
impl Sink for Device {
    fn queued(&self) -> Result<usize, String> {
        Ok(self.input.len())
    }
    fn submit(&mut self, samples: &[i16]) -> Result<(), String> {
        self.input.extend(samples);
        Ok(())
    }
    fn pause(&mut self, paused: bool) -> Result<(), String> {
        self.paused = paused;
        Ok(())
    }
    fn clear(&mut self) -> Result<(), String> {
        self.input.clear();
        self.clears += 1;
        Ok(())
    }
}
impl Device {
    fn consume(&mut self, count: usize) {
        if !self.paused {
            for _ in 0..count {
                // A device underrun is real silence; it must not consume any
                // future PCM that the producer has not submitted yet.
                self.played.push(
                    self.converted
                        .pop_front()
                        .or_else(|| self.input.pop_front())
                        .unwrap_or(0),
                );
            }
        }
    }
    fn convert(&mut self, count: usize) {
        for _ in 0..count {
            if let Some(v) = self.input.pop_front() {
                self.converted.push_back(v);
            }
        }
    }
}
fn pcm(start: usize, end: usize) -> Vec<i16> {
    (start..end).map(|n| (n + 1) as i16).collect()
}
fn queue() -> Queue {
    Queue::new(1000, Policy::default()).unwrap()
}

#[test]
fn prefill_is_nonblocking_and_unavailable_samples_are_not_invented() {
    let mut q = queue();
    let mut d = Device {
        paused: true,
        ..Default::default()
    };
    q.push(0, pcm(0, 40)).unwrap();
    q.service(&mut d, 40).unwrap();
    assert_eq!(q.phase(), Phase::Priming);
    assert!(d.paused);
    assert!(d.input.is_empty());
    q.push(40, pcm(40, 60)).unwrap();
    q.service(&mut d, 60).unwrap();
    assert_eq!(q.phase(), Phase::Running);
    assert!(!d.paused);
    d.consume(60);
    assert_eq!(d.played, pcm(0, 60));
}
#[test]
fn covered_stall_preserves_every_sample_in_order() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 50)).unwrap();
    q.service(&mut d, 50).unwrap();
    // The remaining cushion at the start of this interruption is explicitly50,
    // not inferred just from the configured startup threshold.
    assert_eq!(d.queued().unwrap(), 50);
    d.consume(40);
    q.push(50, pcm(50, 90)).unwrap();
    q.service(&mut d, 90).unwrap();
    assert_eq!(q.stats().expired, 0);
    assert_eq!(d.clears, 0);
    d.consume(50);
    assert_eq!(d.played, pcm(0, 90));
}
#[test]
fn same_prefill_with_smaller_remaining_cushion_can_underrun() {
    let mut q = Queue::new(
        1000,
        Policy {
            max_lateness: Duration::from_millis(20),
            ..Default::default()
        },
    )
    .unwrap();
    let mut d = Device::default();
    q.push(0, pcm(0, 50)).unwrap();
    q.service(&mut d, 50).unwrap();
    d.consume(40);
    q.service(&mut d, 90).unwrap();
    assert_eq!(d.queued().unwrap(), 10);
    d.consume(40); // only10 were covered;30 are genuine silence
    q.push(50, pcm(50, 130)).unwrap();
    q.service(&mut d, 130).unwrap();
    assert_eq!(q.stats().recoveries, 1);
    assert_eq!(q.stats().expired, 30);
    assert_eq!(q.phase(), Phase::Running);
    d.consume(50);
    assert_eq!(&d.played[80..], pcm(80, 130));
}
#[test]
fn empty_input_with_converted_device_cushion_does_not_trigger_recovery() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 50)).unwrap();
    q.service(&mut d, 50).unwrap();
    d.convert(50);
    assert_eq!(d.queued().unwrap(), 0);
    d.consume(40);
    q.push(50, pcm(50, 90)).unwrap();
    q.service(&mut d, 90).unwrap();
    d.consume(50);
    assert_eq!(d.played, pcm(0, 90));
    assert_eq!(d.clears, 0);
}
#[test]
fn future_blocking_program_is_complete_across_many_queue_budgets() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 3000)).unwrap();
    q.service(&mut d, 0).unwrap();
    assert_eq!(d.queued().unwrap(), 100);
    assert_eq!(q.pending_samples(), 2900);
    for now in (10..=3000).step_by(10) {
        d.consume(10);
        q.service(&mut d, now).unwrap();
    }
    assert_eq!(d.played, pcm(0, 3000));
    assert_eq!(q.stats().expired, 0);
}
#[test]
fn genuine_stall_discards_expired_once_and_preserves_future_tail() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 3000)).unwrap();
    q.service(&mut d, 50).unwrap();
    d.consume(350);
    q.service(&mut d, 400).unwrap();
    assert_eq!(q.stats().recoveries, 1);
    assert_eq!(d.clears, 1);
    // Nominal audio trails presentation by50; recovery resumes at350, with
    // the whole future program still available rather than just its newest30.
    assert_eq!(d.input.front(), Some(&351));
    for now in (410..=3050).step_by(10) {
        d.consume(10);
        q.service(&mut d, now).unwrap();
    }
    assert_eq!(&d.played[350..], pcm(350, 3000));
    assert_eq!(d.clears, 1);
}
#[test]
fn recovery_waits_for_catchup_and_does_not_repeatedly_clear_device() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 50)).unwrap();
    q.service(&mut d, 50).unwrap();
    d.consume(350);
    q.service(&mut d, 400).unwrap();
    q.service(&mut d, 410).unwrap();
    assert_eq!(q.phase(), Phase::Recovering);
    assert!(d.paused);
    assert_eq!(d.clears, 1);
    q.push(50, pcm(50, 470)).unwrap();
    q.service(&mut d, 420).unwrap();
    assert_eq!(q.phase(), Phase::Running);
    assert_eq!(d.clears, 1);
    assert_eq!(d.input.front(), Some(&371));
}
#[test]
fn pause_does_not_age_retained_pcm_or_future_programs() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 3000)).unwrap();
    q.service(&mut d, 0).unwrap();
    d.consume(20);
    q.service(&mut d, 20).unwrap();
    q.set_paused(&mut d, true).unwrap();
    let before = (q.stats(), q.pending_samples(), d.queued().unwrap());
    // Arbitrary host time passes, but the supplied engine presentation position
    // is frozen. No wall-clock input enters the policy.
    d.consume(60_000);
    q.service(&mut d, 20).unwrap();
    assert_eq!(
        before,
        (q.stats(), q.pending_samples(), d.queued().unwrap())
    );
    q.set_paused(&mut d, false).unwrap();
    for now in (30..=3000).step_by(10) {
        d.consume(10);
        q.service(&mut d, now).unwrap();
    }
    assert_eq!(d.played, pcm(0, 3000));
    assert_eq!(q.stats().expired, 0);
}
#[test]
fn explicit_resets_allow_new_epochs_and_discard_only_presentation_state() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 500)).unwrap();
    q.service(&mut d, 0).unwrap();
    q.set_paused(&mut d, true).unwrap();
    q.reset(&mut d, 1200).unwrap();
    q.push(1200, pcm(1200, 1250)).unwrap();
    q.service(&mut d, 0).unwrap();
    assert!(d.input.is_empty());
    q.set_paused(&mut d, false).unwrap();
    q.service(&mut d, 0).unwrap();
    d.consume(50);
    assert_eq!(d.played, pcm(1200, 1250));
    assert_eq!(q.stats().resets, 1);
}
#[test]
fn overflow_and_input_gaps_are_errors_not_silent_future_truncation() {
    let mut q = Queue::new(
        1000,
        Policy {
            max_pending: Duration::from_millis(200),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(q.push(0, pcm(0, 300)).is_err());
    assert_eq!(q.pending_samples(), 0);
    q.push(0, pcm(0, 100)).unwrap();
    assert!(q.push(101, pcm(101, 120)).is_err());
    assert_eq!(q.pending_samples(), 100);
    assert_eq!(q.stats().expired, 0);
    let mut d = Device::default();
    q.service(&mut d, 10).unwrap();
    assert!(q.service(&mut d, 9).is_err());
}

#[test]
fn discarded_host_stall_time_is_not_inserted_into_the_engine_timeline() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 1000)).unwrap();
    q.service(&mut d, 0).unwrap();
    d.consume(300); // host stall exceeds this engine's100 ms retained interval
    q.service(&mut d, 100).unwrap(); // supplied target explicitly excludes200 ms
    assert_eq!(q.stats().expired, 0);
    assert_eq!(d.input.front(), Some(&101));
    assert_eq!(q.stats().recoveries, 0);
}

#[test]
fn invalid_policy_and_device_errors_do_not_hide_input_loss() {
    assert!(Queue::new(0, Policy::default()).is_err());
    assert!(Queue::new(1, Policy::default()).is_err());
    assert!(
        Queue::new(
            1000,
            Policy {
                prefill: Duration::from_secs(1),
                ..Default::default()
            }
        )
        .is_err()
    );
    struct Broken;
    impl Sink for Broken {
        fn queued(&self) -> Result<usize, String> {
            Ok(0)
        }
        fn submit(&mut self, _: &[i16]) -> Result<(), String> {
            Err("device failure".into())
        }
        fn pause(&mut self, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn clear(&mut self) -> Result<(), String> {
            Ok(())
        }
    }
    let mut q = queue();
    q.push(0, pcm(0, 100)).unwrap();
    assert_eq!(q.service(&mut Broken, 0), Err("device failure".into()));
    assert_eq!(q.pending_samples(), 100);
    assert_eq!(q.stats().submitted, 0);
}

#[test]
fn explicit_debt_rebase_on_pause_keeps_existing_and_future_pcm() {
    let mut q = queue();
    let mut d = Device::default();
    q.push(0, pcm(0, 3000)).unwrap();
    q.service(&mut d, 100).unwrap();
    d.consume(20);
    q.service(&mut d, 120).unwrap();
    q.set_paused(&mut d, true).unwrap();
    q.rebase(5); // engine discarded pending host debt; generated epoch did not reset
    q.set_paused(&mut d, false).unwrap();
    q.service(&mut d, 5).unwrap();
    for elapsed in (10..=2980).step_by(10) {
        d.consume(10);
        q.service(&mut d, 5 + elapsed).unwrap();
    }
    assert_eq!(d.played, pcm(0, 3000));
    assert_eq!(q.stats().expired, 0);
    assert_eq!(q.stats().rebases, 1);
    assert_eq!(q.stats().recoveries, 0);
}
