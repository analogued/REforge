# Repository instructions

- Read and follow `CONTRIBUTING.md` for code style, commit messages, and validation.
- Keep `docs/` focused on maintained implementation and reference documentation
  useful to users and contributors. Add documentation only when an actual need
  arises; an empty `docs/` is fine. Do not create a preset collection of documents.
- Keep discussions, brainstorming, speculative designs, experiments, internal
  audits, retrospectives, session logs, and planning notes outside the source
  repository. Do not move them to another tracked directory to avoid this rule.
  Promote verified findings into concise implementation documentation when useful.
- This repository contains authored project source and documented RE knowledge.
  Original game files and raw generated evidence are external inputs. Keep them
  outside Git unless a specific redistribution basis has been established.
- These instructions must remain usable in a standalone clone. Do not require
  umbrella instructions or local files outside the repository.
- Resolve original data, analysis, captures, and tool paths through configuration.
  Do not hard-code a user's paths or require a particular parent directory layout.
- Preserve original inputs and persistent intermediate evidence. Put reproducible
  build/cache output in the configured working directory.
- Use Rust for the combined application, engine library crates and shared runtime
  code. Use SDL3 for native presentation; supporting tools may use other languages.
- Keep reconstructed engine code under `engines/<engine-id>/`, shared crates under `crates/`,
  and application code under `app/`. Preserve each game's asset, timing and random
  rules; extract demonstrated common facilities rather than imposing one game loop.
- Keep engine IDs distinct from player game IDs. Engine IDs, Cargo package names
  and Rust crate names align; one engine can support multiple games. Shared video
  or sound hardware alone does not establish a shared game engine.
- REforge selects games/releases through `--original <file-or-directory>`. Keep
  verified filename/size/hash records in the application detection catalog and
  application options in its option catalog. Check engine-parser compatibility
  when adding options. Invalid, unavailable and other-game options warn and are
  ignored; essential original-data failures remain errors. Do not add required
  command-line game selectors or use private originals as test fixtures.
- Keep supporting game/engine-specific RE scripts and capture profiles outside
  this source repository in configured private storage by default. Distinguish
  maintained tools intended for future investigations from helpers bounded to
  one question, dataset or milestone; repeated use alone does not decide scope.
  Preserve exact script/configuration copies with archived evidence when promoting
  a separate working version. Include contributor tools in source only through
  a deliberate project decision; public builds/tests must not require private
  tooling or captures.
- Each worktree contains the whole repository and has its own branch. Coordinate
  shared changes, validate affected games and merge focused branches into `main`.
  Independent clones per game are not the development convention.
- Read applicable instructions and validation requirements in each affected
  engine crate. The RE method and external data requirements are game-specific.
- All games require external original data. Do not bundle extracted assets or
  substitute recreated data to make the application standalone.
- Authored code and documentation use `GPL-3.0-or-later`. Keep the full license
  text in `LICENSE`, state it in the README, and include short copyright and
  SPDX license headers in authored code files. Preserve third-party licenses
  and notices; do not replace them with the project license.
