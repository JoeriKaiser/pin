# Changelog

## [2.2.0]

### Fixed
- `claim` no longer reopens finished work. An item in `done`, `closed`, or `cancelled` state is rejected, and `complete` rejects closed and cancelled items while still allowing a `done` item to be re-verified.
- `pin doctor` reports dependencies that can never be satisfied: `dangling_dependency` when the referenced item is missing from the vault, `blocked_by_cancelled` when it was cancelled. Both cases previously left dependents unready with `pin next` returning nothing and saying nothing.
- `pin view` closes connections that connect and then stall, using a 10 second read and write timeout.
- `pin view` answers oversized request bodies with 413 instead of allocating whatever the `Content-Length` header claims. The limit is 1 MiB.

### Changed
- Split the 1,826-line `main` function into `src/cli.rs` and `src/commands/`, one module per command group. Commands return `Result<(), CliError>` and `main` maps failures to exit codes in one place. `process::exit` calls went from 149 to 3.
- `Status::is_active`, which nothing called, is replaced by `Status::is_finished`, covering `done`, `closed`, and `cancelled`.
- README documents how the vault location and project name resolve, including the collision when two checkouts share a directory name.
- Corrected the 2.0.0 entry, which claimed Linux and Windows ARM64 release binaries that the release matrix does not build.

## [2.0.0]

### Changed
- Ported entire codebase from Zig to a high-standard, modular Rust implementation with zero external runtime dependencies.
- Replaced monolithic single-file build with a clean Cargo project structure and compile-time embedded static web assets (`include_str!`).
- Enhanced memory safety, RAII-based atomic file persistence, and robust error recovery.
- Added native comprehensive unit test suites (`cargo test`) with instantaneous execution.
- Cross-platform CI matrix and automated release packaging across Linux (x86_64), macOS (x86_64, ARM64), and Windows (x86_64). Other architectures fall back to a source build.
- Updated source installer fallback to use the Rust/Cargo toolchain.
- Version bumped to 2.0.0.

## [1.3.0]

### Added
- Interactively browse vault proposals via a localhost browser viewer (`pin view` / `pin view-project`) gated by a loopback URL token, featuring offline Markdown rendering, HTML sanitization, and strict CSP headers.
- Versioned front matter with structured integrity diagnostics and conservative atomic repairs through `pin doctor`.
- Archive and unarchive lifecycle commands with resolution metadata and active/archived filters.
- Deterministic tokenized search ranking, JSON scores, and `search --limit`.
- Post-edit validation with rollback and recovery files for malformed edits.
- A checksum-verifying Windows PowerShell installer and native editor fallback.
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
- Viewer rendering libraries bumped to marked 18.0.7 and DOMPurify 3.4.12.
- Statistics include active, archived, and invalid file counts.
- Import validation rejects any front-matter integrity error before copying files.
- JSON tags are emitted as arrays.
- Markdown headings are used as default titles.
- Legacy timestamp filenames receive deterministic derived IDs.
- Nested repository directories resolve to the same project.
