# Mach 3 instructions

- Read the combined repository's `../../AGENTS.md` and `../../CONTRIBUTING.md`,
  then this game's `CONTRIBUTING.md` for additional validation requirements.
- Use Rust for the main implementation. Supporting tools and investigation
  scripts may use whichever language suits their task. Use SDL3 for native
  presentation, input and queued PCM output. Normal animated playback uses one
  PIT timeline for sample/procedural sound, waits and gameplay; keep simulation
  operational when host audio is unavailable. Step/static/custom-period previews
  retain silent nominal stepping.
- Keep `docs/` focused on maintained implementation and reference documentation
  useful to users and contributors. Add documentation only when an actual need
  arises; an empty `docs/` is fine. Do not create a preset collection of documents.
- Keep discussions, brainstorming, speculative designs, experiments, internal
  audits, retrospectives, session logs, and planning notes outside the source
  repository. Do not move them to another tracked directory to avoid this rule.
  Promote verified findings into concise implementation documentation when useful.
- Keep private input provenance, distribution/archive names, checksums, and
  variant comparisons out of the README and repository docs. Record them with
  external private evidence. Repository docs should describe maintained tools,
  implementation, and verified technical findings useful in a standalone clone.
- This engine crate contains authored project source and documented RE knowledge.
  Original game files and raw generated evidence are external inputs. Keep them
  outside Git unless a specific redistribution basis has been established.
- These instructions must remain usable in a standalone clone. Do not require
  umbrella instructions or local files outside the repository.
- Resolve original data, analysis, captures, and tool paths through configuration.
  Do not hard-code a user's paths or require a particular parent directory layout.
- Preserve original inputs and persistent intermediate evidence. Put reproducible
  build/cache output in the configured working directory.
- Keep game-specific runtime code and technical documentation here under
  `engines/mach3/`. Follow the root instructions for supporting RE tool/helper
  placement: investigation scripts and checkpoint profiles are private by default,
  with exact archived copies preserved independently of maintained working tools.
  Extract demonstrated shared application facilities
  into the combined repository's `crates/`; preserve Mach 3 asset formats,
  gameplay, timing, randomness and input behavior.
- The RE method is a game-specific decision. This scaffold does not select one.
  Add project-specific instructions and build/test commands as they are established.
- Authored game code follows the combined repository's licensing decision, which
  remains undecided. Keep full project license text at the repository root once
  chosen. Preserve game dependency notices and third-party license texts in
  `docs/dependencies.md` and the shared SDL crate's notice files.
