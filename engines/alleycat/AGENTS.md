# Alley Cat engine instructions

- Read the combined repository's root `AGENTS.md`, root `CONTRIBUTING.md` and
  this crate's `CONTRIBUTING.md`. They define shared rules and validation.
- Reconstruct the supported executable's behavior in authored Rust, using its
  external graphics and game tables. Keep behavior and evidence tied to the
  input identity documented in `README.md`.
- Preserve native movement/support rules, shared random consumption, actor order
  and timer/retrace/call-count gates. Consult `docs/timing.md` and
  `docs/deviations.md` before changing pacing or intentional behavior differences.
- Verify behavior against controlled original-code evidence before claiming
  fidelity. Distinguish helper comparisons, staged integration and natural
  gameplay; a window smoke test alone does not establish equivalence.
