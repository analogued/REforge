# Repository instructions

- Read and follow `CONTRIBUTING.md` for code style, commit messages, and validation.
- Use Rust for the main implementation. Supporting tools and investigation
  scripts may use whichever language suits their task. Audio analysis is deferred.
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
- This repository contains authored project source and documented RE knowledge.
  Original game files and raw generated evidence are external inputs. Keep them
  outside Git unless a specific redistribution basis has been established.
- These instructions must remain usable in a standalone clone. Do not require
  umbrella instructions or local files outside the repository.
- Resolve original data, analysis, captures, and tool paths through configuration.
  Do not hard-code a user's paths or require a particular parent directory layout.
- Preserve original inputs and persistent intermediate evidence. Put reproducible
  build/cache output in the configured working directory.
- Put game-specific tools here. Move reusable infrastructure to independent
  repositories when a demonstrated common use justifies doing so.
- The RE method is a game-specific decision. This scaffold does not select one.
  Add project-specific instructions and build/test commands as they are established.
- The license is a project-specific decision and has not been chosen. Once chosen,
  keep its full text in `LICENSE`, state it in the README, and add short copyright
  and SPDX license headers to authored code files. Preserve third-party notices.
