# Repository instructions

- Read and follow `CONTRIBUTING.md` for code style, commit messages, and validation.
- Read `docs/goals.md` for the agreed fidelity and platform requirements. Library
  candidates described there are proposals, not selected dependencies.
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
