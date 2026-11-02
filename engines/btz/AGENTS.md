# BTZ engine instructions

Follow the repository AGENTS.md and CONTRIBUTING.md. Read this crate's README.md
for support and architecture, CONTRIBUTING.md for validation, and the linked
DOS references for implemented semantics.

- Implement the original interpreter algorithms over externally supplied data.
  Keep story rules in the original program rather than adding per-game scripts.
- Preserve typed address spaces, 16-bit arithmetic, original predicates and
  pending host operations. Do not silently emulate unimplemented services.
- Support interpreter revisions only from identified inputs and demonstrated
  semantic differences. Do not assign guessed version numbers.
- Tests use synthetic data. Original files, disassembly and comparison captures
  stay outside the source tree and are supplied through explicit paths.
- Keep UI and application detection outside the VM. Diagnostic binaries require
  the dev-tools feature; normal library builds have no SDL dependency.
- Public documentation must stand alone within this repository. Keep current
  implementation/reference facts separate from experiments, proposals and
  capture-specific reports; do not link to local evidence directories or helpers.
