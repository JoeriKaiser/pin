# Changelog

## [2.2.0]

### Removed
- The `review` status. `done` is now the single end status, reached explicitly with `pin complete --evidence`; `closed` and `cancelled` remain the human-acceptance and withdrawal terminals. Nothing ever verified `review`: it was invisible to `pin stats`, rendered identically to `done` in the viewer, ignored by dependency gating, and used only as a parking spot for the commit+PR action.
- `Status::is_terminal`. With `done` folded into the end states, `Status::is_finished` covers every case, so the two overlapping predicates collapse into one.

### Fixed
- `claim` no longer reopens finished work. An item in `done`, `closed`, or `cancelled` state is rejected, and `complete` rejects closed and cancelled items while still allowing a `done` item's evidence to be refreshed.
- `pin doctor` reports dependencies that can never be satisfied: `dangling_dependency` when the referenced item is missing from the vault, `blocked_by_cancelled` when it was cancelled. Both cases previously left dependents unready with `pin next` returning nothing and saying nothing.
- `pin view` closes connections that connect and then stall, using a 10 second read and write timeout.
- `pin view` answers oversized request bodies with 413 instead of allocating whatever the `Content-Length` header claims. The limit is 1 MiB.
- `pin stats` counts completed work. Items parked in the retired `review` status were silently dropped from every status count.

### Changed
- An agent run now ends by completing its item to `done` with the evidence it extracted, instead of parking it in `review` and releasing the claim.
- Commit + PR requires a completed item and returns it to `in_progress` for the duration of the run, then completes it back to `done` with the PR reference. Finished work no longer has a second staging status to sit in.
- Split the 1,826-line `main` function into `src/cli.rs` and `src/commands/`, one module per command group. Commands return `Result<(), CliError>` and `main` maps failures to exit codes in one place. `process::exit` calls went from 149 to 3.
- `Status::is_active`, which nothing called, is replaced by `Status::is_finished`, covering `done`, `closed`, and `cancelled`.
- The viewer's "Done / Review" board column is now "Done", and the duplicated review action block in the inspector is gone.
- README documents how the vault location and project name resolve, including the collision when two checkouts share a directory name.
- Corrected the 2.0.0 entry, which claimed Linux and Windows ARM64 release binaries that the release matrix does not build.

### Compatibility
- Records written before this change still carry `status: "review"`. They parse as `done` and are rewritten as `done` on their next mutation, so existing vaults need no migration. The `review` name is still accepted on the CLI and resolves to `done`.

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
