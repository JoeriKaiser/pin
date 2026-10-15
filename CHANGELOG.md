# Changelog

## [Unreleased]

## [3.5.0]

### Added
- Unified Server-Sent Events (SSE) stream on `GET /{token}/events` delivering live updates to the web viewer with zero polling overhead and 15-second keepalive frames.
- Low-overhead vault mtime watcher detecting terminal CLI operations and broadcasting changes to browser tabs.
- Linux ARM64 and Windows ARM64 prebuilt release targets.
- Recorded run transcripts can be exported to sanitized fixtures and replayed as a scripted agent (`tests/fixtures/`).

### Fixed
- Board status and Commit + PR button state drift in the web viewer.
- Card click routing race condition where opening a card could select the first card in the backlog.
- Missing fsync before atomic rename (`sync_all`), preventing 0-byte corrupted files on system crash.
- Destructive worktree cleanup in `pin sync` preserves uncommitted changes unless `--force` is passed.
- Rejects dangling dependency IDs during `pin depend`.
- Escapes carriage returns (`\r`), tabs, and quotes in YAML frontmatter strings.
- `pin view` no longer stalls its startup on `gh pr list` lookup.

### Changed
- Version metadata reports reality: release builds stamp the release tag, source installs resolve the latest release tag before building, and `Cargo.toml` carries a `0.0.0` placeholder.
- README drops the zero-network-requests claim; the update check talks to GitHub.

## [3.4.0]

### Added
- Commit + PR runs babysit their pull request: the prompt tells the agent to watch CI and fix failures until checks are green, so the run ends on a green PR or a recorded blocker. `pin view` flushes stdout on every response, and the Windows view smoke test waits longer for startup.

### Removed
- The `review` status. `done` is now the single end status, reached explicitly with `pin complete --evidence`; `closed` and `cancelled` remain the human-acceptance and withdrawal terminals. Nothing ever verified `review`: it was invisible to `pin stats`, rendered identically to `done` in the viewer, ignored by dependency gating, and used only as a parking spot for the commit+PR action.
- `Status::is_terminal`. With `done` folded into the end states, `Status::is_finished` covers every case, so the two overlapping predicates collapse into one.

### Fixed
- `pin stats` counts completed work. Items parked in the retired `review` status were silently dropped from every status count.

### Changed
- An agent run now ends by completing its item to `done` with the evidence it extracted, instead of parking it in `review` and releasing the claim.
- Commit + PR requires a completed item and returns it to `in_progress` for the duration of the run, then completes it back to `done` with the PR reference. Finished work no longer has a second staging status to sit in.
- The viewer's "Done / Review" board column is now "Done", and the duplicated review action block in the inspector is gone.

### Compatibility
- Records written before this change still carry `status: "review"`. They parse as `done` and are rewritten as `done` on their next mutation, so existing vaults need no migration. The `review` name is still accepted on the CLI and resolves to `done`.

## [3.3.1]

### Added
- Quick archive on board cards: cards in the `done` and `review` columns archive from the board, and the inspector can archive closed items and unarchive archived ones, backed by viewer archive and unarchive actions with unit and trajectory tests.

### Fixed
- Board cards move columns as soon as a run starts or a transition succeeds. Reconciliation used to replace a card inside its old parent when the render key changed, leaving it in the backlog column until a reload; run and commit+PR dispatches now set the new status optimistically and re-render board and list immediately, and ID lookup accepts short prefixes everywhere.

## [3.3.0]

### Added
- Release binaries report the tag they were built from. `build.rs` resolves the version from `PIN_VERSION`, the checked-out tag, or the Cargo manifest, and the release workflow stamps `PIN_VERSION` from `github.ref_name`.

### Fixed
- Streamed assistant prose keeps its shape: chunks are concatenated instead of joined with newlines, whitespace-only tokens survive extraction, and completed tickets render handoff fields as GFM markdown with styling for headings, lists, links, quotes, and code.
- The Commit + PR button appears the moment a run finishes; board drags and modal actions refresh what they changed, and returning to the board re-renders its cards.
- Viewer sluggishness and re-render glitches: `GET /data.json` answers with a non-blocking ETag 304 instead of a synchronous Git fetch, board cards reconcile in place instead of being rebuilt, the first paint populates from `state.shown`, the board is the default route on refresh, and starting a run from the board keeps the board view.

## [3.2.0]

### Added
- `pin sync` reconciles tickets with Git: it inspects the repository, finds committed items whose work reached main, and marks them merged, so the board and inspector show a "Merged into main" badge and the commit action resolves instead of lingering open. `pin view` gains a Sync Git button that runs it.

### Fixed
- The backlog column's new task input opens again: quick-add triggers truncate and size with flex, and the quick-idea button matches the quick-add trigger.

## [3.1.0]

### Added
- Idea mode. Idea tickets gain an interactive run mode where messages continue the conversation (`RunMode::Idea`, `POST /run/:id/message`), and `pin to-spec` (or the viewer's to-spec action) transforms an idea into a spec ticket.
- `pin update` checks GitHub releases with semver comparison, resolves the platform asset, verifies its SHA-256 before replacing the binary, and reports from every terminal command and the viewer's update button.

### Changed
- The viewer frontend is split into 15 ES modules with a unidirectional dependency graph and zero cycles, served from an embedded asset table; `app.js` is an 8-line entry point. `tests/viewer_trajectory.js` covers DOM behavior, ACP streams, the tool inspector, and run dispatch, and runs in CI.

## [3.0.1]

### Fixed
- The inspector rail puts proposal actions above the handoff card so they are visible without scrolling, prioritizes verification evidence over progress notes, and gives the handoff blocks compact styling with scroll limits.

## [3.0.0]

### Added
- ACP agent execution. `pin view` can run items with a configured agent command (`--acp-command`), streams session updates into the agent drawer, writes every notification to a run transcript under `runs/`, and completes the item with the summary and verification extracted from the run.
- Worktree execution: runs happen on a fresh git worktree from the main branch under `.pin_worktrees/<id>`, and a Commit + PR quick action commits the worktree and opens a pull request for the item.
- Clipboard screenshots paste directly into the ticket description editor as image references.

### Changed
- The viewer gained a cohesive design system of tokens and component styles, and the trajectory timeline caches reopened tool calls and buffers SSE output so large transcripts open without sluggishness.
- ACP session prompts guard against archiving items that have active runs.

## [2.2.0]

### Fixed
- `claim` no longer reopens finished work. An item in `done`, `closed`, or `cancelled` state is rejected, and `complete` rejects closed and cancelled items while still allowing a `done` item's evidence to be refreshed.
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
