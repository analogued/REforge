// Copyright (c) 2026 analogued. Project license has not been selected.
#![cfg(feature = "blip")]

use reforge_audio::{
    PIT_HZ,
    render::{BlipRenderer, Config, Error, SignalRenderer},
    speaker::PcSpeaker,
};

fn renderer(rate: u32, chunk: u32) -> BlipRenderer {
    BlipRenderer::new(
        Config {
            clock_hz: PIT_HZ as u32,
            sample_rate: rate,
            chunk_ticks: chunk,
        },
        0,
        0,
    )
    .unwrap()
}

fn pattern(rate: u32, chunk: u32, finishes: &[u64]) -> Vec<i16> {
    let mut renderer = renderer(rate, chunk);
    let mut pcm = Vec::new();
    let mut output = |block: &[i16]| {
        pcm.extend_from_slice(block);
        Ok(())
    };
    let mut boundary = 0;
    for tick in (0..PIT_HZ).step_by(77) {
        while finishes.get(boundary).is_some_and(|end| *end <= tick) {
            renderer
                .finish_until(finishes[boundary], &mut output)
                .unwrap();
            boundary += 1;
        }
        renderer
            .level_at(
                tick,
                if (tick / 77) & 1 == 0 { 8192 } else { 0 },
                &mut output,
            )
            .unwrap();
    }
    while let Some(&end) = finishes.get(boundary) {
        renderer.finish_until(end, &mut output).unwrap();
        boundary += 1;
    }
    renderer.finish_until(PIT_HZ, &mut output).unwrap();
    assert_eq!(renderer.samples_written(), u64::from(rate));
    assert_eq!(pcm.len(), rate as usize);
    pcm
}

#[test]
fn synthesis_and_finish_partitions_preserve_continuous_pcm_at_several_rates() {
    for rate in [8000, 32000, 44100, 48000, 96000] {
        let one = pattern(rate, 11931, &[]);
        assert_eq!(
            one,
            pattern(
                rate,
                137,
                &[1, 77, 137, 138, 199, 32768, 65536, 131072, 301017]
            )
        );
        assert!(one.iter().any(|sample| *sample != 0));
    }
}

#[test]
fn long_silent_or_held_intervals_split_without_a_graphics_body_limit() {
    let mut renderer = renderer(48000, 11931);
    let mut count = 0;
    let mut max_block = 0;
    renderer
        .finish_until(PIT_HZ * 20, &mut |samples| {
            assert!(samples.iter().all(|sample| *sample == 0));
            count += samples.len();
            max_block = max_block.max(samples.len());
            Ok(())
        })
        .unwrap();
    assert_eq!(count, 960000);
    assert!(max_block <= 480);
    assert_eq!(renderer.tick(), PIT_HZ * 20);
}

fn device_render(chunk: u32, rate: u32) -> (PcSpeaker, Vec<i16>) {
    let mut device = PcSpeaker::new(0, [true; 3]);
    let mut renderer = renderer(rate, chunk);
    let mut pcm = Vec::new();
    let mut output = |samples: &[i16]| {
        pcm.extend_from_slice(samples);
        Ok(())
    };
    let mut writes = vec![
        (0, 0x43, 0xb6),
        (0, 0x42, 7),
        (0, 0x42, 0),
        (0, 0x61, 3),
        (13001, 0x61, 1),
        (14003, 0x61, 3),
        (16001, 0x43, 0x90),
    ];
    // Repeated one-shot PWM counts, including equal writes, are executed by
    // the device; the renderer sees only the resulting drive changes.
    for index in 0..300 {
        writes.push((
            16001 + index * 60,
            0x42,
            if index % 3 == 0 { 1 } else { 64 },
        ));
    }
    writes.push((35000, 0x61, 0));
    let mut next = 0;
    let end = 100000;
    loop {
        let target = writes.get(next).map_or(end, |entry| entry.0);
        while device.pit().tick() < target {
            let events = device.advance_to(target).unwrap();
            if let Some(edge) = events.drive {
                renderer
                    .level_at(edge.tick, if edge.high { 8192 } else { 0 }, &mut output)
                    .unwrap();
            }
        }
        while let Some(&(tick, port, value)) = writes.get(next).filter(|entry| entry.0 == target) {
            let events = device.write_port(port, value).unwrap();
            if let Some(edge) = events.drive {
                assert_eq!(edge.tick, tick);
                renderer
                    .level_at(edge.tick, if edge.high { 8192 } else { 0 }, &mut output)
                    .unwrap();
            }
            next += 1;
        }
        if target == end {
            break;
        }
    }
    renderer.finish_until(end, &mut output).unwrap();
    (device, pcm)
}

#[test]
fn odd_tones_gate_changes_and_pwm_reload_render_independently_of_chunking() {
    let (device, pcm) = device_render(11931, 48000);
    assert_eq!((device.clone(), pcm.clone()), device_render(499, 48000));
    assert_eq!(device, device_render(1000, 44100).0);
    assert!(pcm.iter().any(|sample| *sample != 0));
}

#[test]
fn invalid_inputs_are_recoverable_but_output_failure_stops_synthesis() {
    let mut renderer = renderer(48000, 11931);
    renderer.level_at(100, 8192, &mut |_| Ok(())).unwrap();
    assert!(renderer.level_at(99, 0, &mut |_| Ok(())).is_err());
    assert!(renderer.level_at(200, 8193, &mut |_| Ok(())).is_err());
    assert_eq!(renderer.tick(), 100);
    renderer.finish_until(200, &mut |_| Ok(())).unwrap();
    let error = renderer
        .finish_until(30000, &mut |_| {
            Err(Error::Io(std::io::Error::other("synthetic output failure")))
        })
        .unwrap_err();
    assert!(matches!(error, Error::Io(_)));
    assert!(matches!(
        renderer.finish_until(30001, &mut |_| Ok(())),
        Err(Error::Stopped)
    ));
    assert!(matches!(
        renderer.level_at(30001, 0, &mut |_| Ok(())),
        Err(Error::Stopped)
    ));
}
