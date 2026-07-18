# Changelog

## Unreleased

### Added
- Interactively browse vault proposals via a localhost browser viewer (`pin view` / `pin view-project`) gated by a loopback URL token, featuring offline Markdown rendering, HTML sanitization, and strict CSP headers.

- Versioned front matter with structured integrity diagnostics and conservative atomic repairs through `pin doctor`.
- Archive and unarchive lifecycle commands with resolution metadata and active/archived filters.
- Deterministic tokenized search ranking, JSON scores, and `search --limit`.
- Post-edit validation with rollback and recovery files for malformed edits.
- A checksum-verifying Windows PowerShell installer and native editor fallback.
- Shared CLI argument parsing helpers and Zig parser/search unit tests.
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

- Version bumped to 0.4.0.
- Statistics include active, archived, and invalid file counts.
- Import validation rejects any front-matter integrity error before copying files.
- JSON tags are emitted as arrays.
- Markdown headings are used as default titles.
- Legacy timestamp filenames receive deterministic derived IDs.
- Nested repository directories resolve to the same project.
- The source installer fallback is pinned to the latest release and Zig 0.16.0.
