# Recommended Solutions and Implementation Roadmap

This document converts the current dogfooded pins and project review findings into an implementation plan. It is planning only; no behavior or source code is changed by this document.

## Goals and constraints

All work should preserve the qualities that make `pin` useful:

- zero runtime dependencies;
- a plain-Markdown vault that remains understandable and editable without `pin`;
- deterministic output for scripts and coding agents;
- backward compatibility with legacy timestamp-named files;
- a single distributable `main.zig`, because the source installer currently downloads that file directly;
- conservative, explicit mutation of user data.

## Recommended priority

1. **Vault integrity and diagnostics** — highest priority. Silent omission or corruption undermines the registry's core promise.
2. **Archive and resolution lifecycle** — keeps active context trustworthy without deleting history.
3. **Windows installation path** — closes an existing distribution gap for binaries already produced by CI.
4. **Unified CLI argument parsing** — not a user-facing blocker, but should precede adding several new commands and flags.
5. **Ranked search** — valuable as vaults grow, but validate that current result sets are large enough to justify it.

The argument-parsing refactor is listed fourth by value but should be implemented early as a small, behavior-preserving preparatory pull request.

---

## 1. Vault integrity and front-matter hardening

Related pin: **Vault integrity and lifecycle** (`f66c6163f231`).

### Problem

`collect_ideas` currently skips unreadable files with `catch continue`, and the front-matter parser silently substitutes values such as `0` or `unspecified` when input is malformed. A proposal can therefore disappear or degrade without explaining why. `pin edit` also allows a valid proposal to be edited into an invalid state without post-edit validation.

### Recommended solution

Keep the deliberately small front-matter format rather than introducing a general YAML dependency. Formalize it as a constrained, versioned schema and make parsing diagnostic rather than silent.

Add an optional schema field:

```yaml
schema: 1
```

Absence of `schema` means the existing legacy schema. Legacy files remain readable and are never changed automatically unless the user requests repair.

Refactor parsing around a result similar to:

```text
ParseResult
  meta: optional IdeaMeta
  issues: list of Issue
  body_start: optional byte offset

Issue
  code: stable machine-readable code
  severity: info | warning | error
  filename
  line
  field: optional field name
  message
  repairable: boolean
```

Stable issue codes are important for JSON consumers; prose messages alone should not be the API. Suggested initial codes:

- `missing_front_matter`
- `unterminated_front_matter`
- `duplicate_field`
- `missing_required_field`
- `invalid_integer`
- `invalid_kind`
- `invalid_priority`
- `missing_id`
- `id_filename_mismatch`
- `duplicate_id`
- `unreadable_file`

Normal commands may continue excluding unusable files, but the shared scanner should count and expose skipped files. `stats` should report malformed/unreadable counts rather than presenting an incomplete vault as fully healthy.

### Required validation

Validate at least:

- opening and closing front-matter delimiters;
- required `project` and `title` values;
- numeric `timestamp` and `created_at_ns` values;
- known `kind` and `priority` values;
- duplicate front-matter keys;
- explicit IDs matching the expected 12-character form;
- duplicate IDs across files;
- explicit ID versus filename-stem mismatches.

Unknown front-matter fields should be preserved and reported only when they are unsafe. This permits forward compatibility and user annotations.

### Edit-time prevention

After the editor exits successfully, `pin edit` should parse the edited file before accepting it:

1. Preserve the original bytes before launching the editor.
2. Parse and validate the edited result.
3. If valid, keep it.
4. If invalid, save the rejected edit to a non-scanned recovery file, restore the original atomically, print diagnostics and the recovery path, then return non-zero.

A recovery file is preferable to simply discarding the user's invalid edit. Its extension should not be `.md`, so the vault scanner cannot mistake it for a proposal.

### Atomic writes

All generated repairs and metadata rewrites should:

1. write a uniquely named temporary file in the same directory;
2. flush and close it;
3. replace the original with a rename;
4. leave temporary files with a non-`.md` extension so interrupted operations do not enter the vault.

Do not overwrite the original until the replacement is complete.

### Acceptance criteria

- Malformed and unreadable files no longer disappear without a diagnostic path.
- Existing valid and legacy vault files produce unchanged `list`, `search`, and `context` results.
- Invalid edits do not replace the last valid proposal, and the attempted edit remains recoverable.
- Parser diagnostics have stable codes and deterministic JSON serialization.
- Unit tests cover malformed delimiters, duplicate fields, invalid values, escaping, Unicode and legacy metadata.

---

## 2. `pin doctor` and safe repair

Related pin: **Vault integrity and lifecycle** (`f66c6163f231`).

### Command design

```text
pin doctor [--repair] [--strict] [--format json|plain]
```

The read-only default is important: merely inspecting a vault must never mutate it.

The human report should group issues by file and conclude with a summary. Suggested JSON shape:

```json
{
  "healthy": false,
  "files_scanned": 12,
  "issues": [
    {
      "code": "missing_id",
      "severity": "warning",
      "filename": "legacy.md",
      "line": 1,
      "field": "id",
      "message": "Legacy proposal has no explicit ID",
      "repairable": true
    }
  ],
  "summary": {
    "errors": 0,
    "warnings": 1,
    "repairable": 1
  }
}
```

### Exit codes

Use a documented contract:

- `0`: no unresolved errors;
- `1`: one or more integrity errors remain;
- `2`: invocation or runtime failure.

Warnings should not fail ordinary automation. `--strict` should promote warnings to exit code `1` for CI users who require a fully normalized vault.

### Safe repairs

The initial `--repair` scope should be intentionally conservative:

- add a deterministic ID to a legacy file missing one;
- add `schema: 1` when the rest of the file is already valid;
- normalize recognized kind or priority values that differ only by case;
- canonicalize metadata written by `pin` while preserving unknown fields and the Markdown body byte-for-byte.

Do **not** automatically repair:

- missing project names;
- missing titles unless a future explicit option derives one from a heading;
- duplicate IDs;
- arbitrary invalid enum values;
- malformed bodies or ambiguous delimiters;
- filename/ID mismatches by silently renaming files.

These require user intent. Report a recommended command or manual action instead.

When `--repair` is used, scan once, apply safe fixes atomically, then scan again and report the final state. The output should distinguish repaired issues from unresolved issues.

### Tests

Add fixture-based cases for:

- a healthy current-schema vault;
- valid legacy files;
- absent and unterminated front matter;
- duplicate IDs;
- unreadable files where supported by the host OS;
- invalid kind, priority and timestamps;
- a mixed vault where repair succeeds for some files and leaves others untouched;
- repair idempotence: a second `pin doctor --repair` makes no changes.

### Acceptance criteria

- `pin doctor` is non-mutating without `--repair`.
- Every skipped `.md` vault file appears in the report.
- Repairs are atomic, conservative and idempotent.
- Human and JSON reports describe the same issues.
- Exit codes follow the documented contract.

---

## 3. Archive and resolution lifecycle

Related pin: **Vault integrity and lifecycle** (`f66c6163f231`).

### Problem

`pin rm` permanently deletes a proposal. Implemented, rejected or superseded proposals must therefore remain in active context forever or lose their decision history.

### Recommended storage model

Keep archived proposals in place and add front-matter fields:

```yaml
archived_at: 1784220005
resolution: "implemented"
resolution_note: "Shipped in v0.4.0"
```

Recommended `resolution` values:

- `implemented`
- `rejected`
- `superseded`
- `stale`

`resolution_note` remains free text. Presence of `archived_at` determines archived status; this avoids duplicating status in another field.

Keeping the file in place is preferable to an archive directory because it preserves filenames and selectors, creates a small Git diff, and makes unarchiving straightforward.

### Commands

```text
pin archive <id|prefix|filename>
            [--resolution implemented|rejected|superseded|stale]
            [--note <text>]
            [--format json|plain]

pin unarchive <id|prefix|filename> [--format json|plain]
```

Behavior changes:

- `list`, `list-project`, `search` and `context` exclude archived proposals by default;
- `--archived` returns archived proposals only;
- `--all` includes both active and archived proposals;
- `read`, `edit`, `archive`, `unarchive` and selector resolution can find either state;
- `stats` reports active, archived and total counts;
- duplicate-title rejection applies to active proposals; an archived title may be proposed again;
- `rm` remains available as explicit, irreversible deletion.

Archive and unarchive should use the same canonical front-matter writer and atomic replacement mechanism as doctor repairs.

### Acceptance criteria

- Archiving removes a proposal from ordinary context without deleting it.
- Unarchiving restores it with the same stable ID and original content.
- Resolution metadata survives export/import and direct edits.
- Filters have consistent behavior across list, search and context.
- Legacy files remain active unless explicitly archived.

---

## 4. Unified CLI argument parsing

Related pin: **Unified CLI argument parsing** (`346eb89948dc`).

### Problem

Each subcommand manually implements flag iteration, missing-value checks, format validation and unknown-argument errors. The duplication has already produced drift: `stats` uses a positional argument-count check while most commands use a flag loop. Adding doctor, archive, unarchive and lifecycle filters would multiply that boilerplate.

### Recommended solution

Create a deliberately small internal parser in `main.zig`; do not add a dependency or build a general-purpose getopt implementation.

It only needs to support the project's existing grammar:

- long flags such as `--format`;
- boolean flags;
- values supplied as the next argument;
- positional arguments;
- command-specific allowed flags;
- repeated-flag policy;
- standardized missing-value and unknown-argument diagnostics.

Useful shared helpers include:

```text
requireFlagValue(args, index, flag)
parseFormatArg(value, allowedFormats, command)
parseSelectorCommand(args, command)
openVaultOrExit(path)
resolveSelectorOrExit(selector)
loadIdeaId(filename)
```

`read`, `edit` and `rm` should share selector parsing and resolution while retaining their different output defaults and file-lifetime behavior.

Treat this as a mechanical refactor:

- preserve command syntax;
- preserve default formats;
- preserve stdout/stderr separation;
- preserve non-zero failures;
- avoid combining it with behavioral changes in the same pull request.

### Tests

Before refactoring, expand argument-error coverage for every command:

- missing flag values;
- unknown flags;
- extra positional arguments;
- invalid formats;
- repeated formats;
- `stats` following the same rules as other commands.

Exact wording may be standardized, but tests should verify stable error categories and exit status.

### Acceptance criteria

- Existing positive CLI tests remain unchanged and pass.
- Every command uses the shared value and format parsing helpers.
- `stats` no longer has bespoke positional-count parsing.
- Adding a value flag to a new command does not require another copy of the missing-value logic.
- The project remains a single-source Zig executable.

---

## 5. Windows installation and editor support

Related pin: **Windows installation path** (`0c9c1a9a1776`).

### Problem

Release CI produces Windows AMD64 and ARM64 executables and checksums, but the documented installer is POSIX shell only. Windows users have no supported installation flow. `pin edit` also defaults to `vi` when `EDITOR` and `VISUAL` are absent, which is unsuitable on a standard Windows installation.

### Recommended installer

Add `install.ps1` with explicit parameters to make it testable:

```powershell
param(
    [string] $Version = "latest",
    [string] $InstallDir = "$env:LOCALAPPDATA\Programs\pin",
    [string] $BaseUrl = "https://github.com/JoeriKaiser/pin/releases"
)
```

The script should:

1. detect AMD64 versus ARM64 using .NET runtime architecture information;
2. construct the matching release asset and checksum URLs;
3. download both into a temporary directory;
4. verify SHA-256 with `Get-FileHash` before execution or installation;
5. install as `$InstallDir\pin.exe`;
6. add the directory to the user PATH idempotently and case-insensitively;
7. explain that a new terminal may be required;
8. clean up temporary files in `finally`;
9. fail clearly on unsupported architectures or unavailable assets.

Start with release binaries only. Source fallback can be added later if Windows demand justifies maintaining Zig bootstrap logic in two installers.

### Documentation

Document both:

```powershell
irm https://raw.githubusercontent.com/JoeriKaiser/pin/main/install.ps1 | iex
```

and a manual download/checksum procedure for users who do not run remote scripts directly.

### Editor behavior

When neither `EDITOR` nor `VISUAL` is set:

- use `notepad` on Windows builds;
- use `vi` on Unix-like builds.

Environment overrides continue to take precedence.

### CI strategy

Avoid a smoke test coupled nondeterministically to whatever release is currently `latest`. The installer parameters should allow CI to test against a pinned version or a local/mock asset base URL.

A Windows CI job should verify:

- AMD64 asset selection;
- checksum success and checksum-failure rejection;
- installation to a temporary directory;
- idempotent PATH handling;
- the installed `pin.exe --version` result.

Before prioritizing implementation, inspect Windows asset download counts through the GitHub API to validate demand. The release assets should remain available regardless.

### Acceptance criteria

- A clean Windows AMD64 or ARM64 user can install a verified binary without WSL.
- Checksum failure prevents installation.
- Re-running the installer is safe and does not duplicate PATH entries.
- `pin edit` works on a default Windows installation.
- README instructions cover one-line and manual installation.

---

## 6. Ranked and tokenized search

Related pin: **Ranked search results** (`bfb6653cc5dd`).

### Problem

Search currently applies one case-insensitive substring test to the entire file and then returns results by recency. A proposal with the query in its title is indistinguishable from one containing an incidental body mention. Multi-word queries only match when the complete phrase appears verbatim. This weakens the deduplication step prescribed by `SKILL.md`.

### Recommended solution

Preserve deterministic, dependency-free search. Do not introduce fuzzy libraries, embeddings or an index at current vault scale.

Tokenize a query into whitespace-separated terms and require every term to match at least one searchable field. Calculate an explainable score such as:

- title term match: `8` points;
- tag term match: `4` points;
- body term match: `1` point;
- exact full-query title match: substantial bonus;
- exact phrase or word-boundary match: small bonus.

Sort using stable tiebreakers:

1. score descending;
2. priority descending;
3. timestamp/creation time descending;
4. filename for deterministic final ordering.

Add:

```text
pin search <query> [--limit <n>] ...
```

JSON results may include a numeric `score` as an additive field. Plain and table outputs only need the improved ordering.

Search should operate on parsed title, tags and Markdown body separately rather than assigning front-matter syntax accidental body weight. Archived filtering must follow the lifecycle rules above.

### Validation before implementation

Replay representative searches against the current vault. If most searches produce three or fewer candidates, ranking has little immediate value and should remain low priority. The tokenized AND behavior may still be worthwhile because it improves multi-term discovery without adding dependencies.

### Acceptance criteria

- A title match ranks above an incidental body-only match.
- Multi-term queries match terms in different fields or positions.
- Ordering is deterministic across runs.
- `--limit` is applied after ranking.
- Existing single-term matches remain discoverable.
- JSON output remains backward compatible through additive fields only.

---

## Delivery sequence

Use small, independently reviewable pull requests:

1. **CLI regression coverage** for argument and malformed-vault behavior.
2. **Mechanical argument-parsing refactor** with no command behavior changes.
3. **Structured parser diagnostics** and schema representation.
4. **Read-only `pin doctor`** with human/JSON reports and exit codes.
5. **Conservative `doctor --repair`** plus atomic front-matter rewriting.
6. **Post-edit validation and recovery** using the same parser/writer.
7. **Archive/unarchive lifecycle** and active/archived filters.
8. **Windows installer and editor fallback**, independently deliverable earlier if desired.
9. **Tokenized/ranked search**, only after validating result-set size.

Every pull request should update `CHANGELOG.md`, `README.md` where behavior changes, `SKILL.md` when agent workflows change, and `tests/cli.sh`. Parser-focused work should also add Zig unit tests, preferably as `test` blocks that can run with `zig test main.zig` while preserving the single-file distribution model.

## Explicit non-goals

To keep the project coherent, this roadmap does not recommend:

- replacing Markdown storage with a database;
- implementing a general YAML parser;
- adding a third-party CLI parsing dependency;
- moving source into multiple runtime-required files while the installer expects one `main.zig`;
- fuzzy search, embeddings or a background search index;
- automatic repair when the intended value cannot be inferred safely;
- removing permanent deletion—`rm` remains an explicit escape hatch.
