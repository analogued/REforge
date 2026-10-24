# Mach 3 engine instructions

- Read the combined repository's root `AGENTS.md`, root `CONTRIBUTING.md` and
  this crate's `CONTRIBUTING.md` for shared rules and validation.
- Reconstruct the supported DOS executable's behavior in authored Rust, using
  its external resources and decoded program tables. Input identities and raw
  comparison evidence belong with the configured private inputs.
- Preserve encounter rules, shared random consumption, partial session resets,
  cached drawing state and original input-poll order. Consult `docs/dos-gameplay.md`
  before changing gameplay or pacing, including its intentional corrections.
- Normal animated playback uses one reconstructed PIT timeline for samples,
  procedural sound, keyboard handlers and foreground waits. Output failure or
  host mute must not change execution. Step/static/custom-period previews retain
  silent nominal stepping; do not confuse their deadlines with live PIT timing.
- Verify behavior against controlled original-code evidence. Distinguish logical
  replay, synthetic tests, native presentation checks and timing/audio comparisons;
  none alone establishes full-game fidelity.
