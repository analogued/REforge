# Repository instructions

- Read the combined repository's root `AGENTS.md`, `CONTRIBUTING.md` and this
  crate's `CONTRIBUTING.md` before game work.
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
- The recreation must require external original game data at runtime. Read the
  original graphics and game tables from the supported executable; do not bundle
  extracted assets or replace them to make the program self-contained.
- These instructions must remain usable in a standalone clone. Do not require
  umbrella instructions or local files outside the repository.
- Resolve original data, analysis, captures, and tool paths through configuration.
  Do not hard-code a user's paths or require a particular parent directory layout.
- Preserve original inputs and persistent intermediate evidence. Put reproducible
  build/cache output in the configured working directory.
- Put game-specific tools here. Shared REforge infrastructure belongs in root
  `crates/` when demonstrated reuse justifies extraction.
- The RE method is a game-specific decision. This scaffold does not select one.
  Add project-specific instructions and build/test commands as they are established.
- The repository-wide REforge license has not been chosen. Once chosen,
  keep its full text in root `LICENSE`, state it in the README, and add short copyright
  and SPDX license headers to authored code files. Preserve third-party notices.
