# Changelog

## Unreleased

### Added

- Repository-aware project resolution with `PIN_PROJECT` and `.pin-project` overrides.
- Stable proposal IDs and unambiguous ID-prefix selectors.
- `pin context --limit <n>` for compact agent session context, with domain filtering and `--group kind`.
- JSON, table, and plain output contracts with TTY-aware defaults.
- Duplicate-title detection and optional priority metadata.
- Required `technical`, `product`, `business`, or `project` domains for new pins.
- Domain-aware JSON, tables, filters, context grouping, and statistics with legacy `unspecified` compatibility.
- A proactive agent-curation protocol with evidence gates, deduplication, per-session budgets, and domain-specific guidance.
- Repository-local, Git-shareable vaults through `pin init --local`.
- Vault import and export commands.
- `--help`, `--version`, CLI behavior tests, and an MIT license.

### Changed

- JSON tags are emitted as arrays.
- Markdown headings are used as default titles.
- Legacy timestamp filenames receive deterministic derived IDs.
- Nested repository directories resolve to the same project.
- The source installer fallback is pinned to the latest release and Zig 0.16.0.
