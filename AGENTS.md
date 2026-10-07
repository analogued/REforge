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
- Use Rust for the combined application, game library crates and shared runtime
  code. Use SDL3 for native presentation; supporting tools may use other languages.
- Keep game runtime code under `games/<game-id>/`, shared crates under `crates/`,
  and application code under `app/`. Preserve each game's asset, timing and random
  rules; extract demonstrated common facilities rather than imposing one game loop.
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
  game crate. The RE method and external data requirements are game-specific.
- All games require external original data. Do not bundle extracted assets or
  substitute recreated data to make the application standalone.
- The license is a project-specific decision and has not been chosen. Once chosen,
  keep its full text in `LICENSE`, state it in the README, and add short copyright
  and SPDX license headers to authored code files. Preserve third-party notices.
