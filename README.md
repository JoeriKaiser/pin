# pin

A local-first, Git-backed work ledger for humans and coding agents.

`pin` saves ideas, tasks, bugs, and decisions as Markdown with YAML front matter. A record can move from captured, to planned, to active work, to verified completion. Agents can claim work, leave handoffs, and report evidence. Humans can inspect and steer the same records in the local viewer.

It remains smaller than a hosted issue tracker. The vault is plain Markdown, synchronized through Git, and does not require a server or external tracker.

## Installation

### Linux and macOS

```bash
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/install.sh | sh
```

Supported platforms use checksum-verified release binaries. The source fallback requires Rust 1.70+ (`cargo`).

### Windows

From PowerShell:

```powershell
irm https://raw.githubusercontent.com/JoeriKaiser/pin/main/install.ps1 | iex
```

The installer detects AMD64 or ARM64, verifies the release checksum, installs to `%LOCALAPPDATA%\Programs\pin`, and adds that directory to the user PATH. Open a new terminal after the first install.

To install manually, download the matching `pin-windows-*.exe` and `.sha256` assets from the latest GitHub release, verify with `Get-FileHash -Algorithm SHA256`, rename the executable to `pin.exe`, and place it on your PATH.

## Agent integration

```bash
mkdir -p ~/.agents/skills/pin
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/SKILL.md \
  -o ~/.agents/skills/pin/SKILL.md
```

Pi discovers skill metadata automatically but loads full skill instructions on demand. To make proactive curation an always-on project behavior, add this small trigger to `~/.pi/agent/AGENTS.md`:

```markdown
- For every software project session, load the `pin` skill at the start and follow its proactive improvement-curation protocol throughout the session.
```

At session start, agents can request compact work context grouped by domain:

```bash
pin context --limit 10 --group kind --format plain
```

The bundled skill also defines a proactive curation protocol: agents notice substantial out-of-scope improvements during normal work, apply strict evidence and quality gates, deduplicate them, and add at most one pin per ordinary session.

For active work, agents should use this loop:

```bash
pin context --limit 10 --format plain
pin next --limit 1 --format json
pin claim <id> --actor agent:name
# do the work
pin handoff <id> --actor agent:name --progress "..." --next "..."
pin complete <id> --actor agent:name --evidence "..."
```

Use `pin transition <id> --to blocked --note "..."` when work cannot continue. Claims have a one-hour lease by default and can be renewed with `pin claim`.

Set `PIN_ACTOR` to provide the agent identity for commands that accept `--actor`. Set `PIN_VIEWER_ACTOR` when the local viewer should record a different human identity.

## Quick start

```bash
# The first Markdown heading becomes the title.
pin add '# Lazy widget loading

Load expensive widgets only when they enter the viewport.' \
  --kind product --tags perf,ux --priority high

# JSON is the default when output is piped.
id=$(pin list-project --format plain | awk 'NR == 1 { print $1 }')

# Exact IDs, unambiguous ID prefixes, and legacy filenames all work.
pin read "$id"
pin edit "$(printf %s "$id" | cut -c1-5)"

pin search 'widget' --format table
pin context --limit 10 --group kind --format plain
pin next --limit 1 --format json
```

## Commands

```text
pin init --local [--project <name>] [--format json|plain]
pin add <markdown> --kind technical|product|business|project
                     [--stdin] [--project <name>] [--title <title>]
                      [--tags <csv>] [--priority low|medium|high]
                      [--type idea|task|bug|decision]
                     [--allow-duplicate] [--format json|plain]
pin list [--project <name>] [--tag <name>] [--kind <kind>]
          [--type <type>] [--status <status>] [--claimed-by <actor>] [--ready]
          [--archived|--all] [--format json|table|plain]
pin list-project [--tag <name>] [--kind <kind>] [--type <type>]
                  [--status <status>] [--claimed-by <actor>] [--ready] [--archived|--all]
                  [--format json|table|plain]
pin search <query> [--project <name>] [--tag <name>] [--kind <kind>]
                    [--type <type>] [--status <status>] [--claimed-by <actor>]
                    [--limit <n>] [--archived|--all]
                    [--format json|table|plain]
pin context [--project <name>] [--kind <kind>] [--type <type>]
             [--status <status>] [--limit <n>]
             [--group kind] [--archived|--all] [--format json|plain]
pin next [--project <name>] [--limit <n>] [--format json|plain]
pin transition <id|id-prefix|filename> --to <status> [--actor <name>]
             [--note <text>] [--expect-revision <n>] [--format json|plain]
pin claim <id|id-prefix|filename> [--actor <name>] [--lease <seconds>]
             [--expect-revision <n>] [--format json|plain]
pin release <id|id-prefix|filename> [--actor <name>] [--force]
             [--expect-revision <n>] [--format json|plain]
pin handoff <id|id-prefix|filename> [--actor <name>] [--progress <text>]
             [--next <text>] [--blocker <text>] [--verification <text>]
             [--expect-revision <n>] [--format json|plain]
pin complete <id|id-prefix|filename> --evidence <text> [--actor <name>]
             [--expect-revision <n>] [--format json|plain]
pin close <id|id-prefix|filename> [--actor <name>] [--note <text>]
             [--expect-revision <n>] [--format json|plain]
pin depend <id|id-prefix|filename> <dependency-id> [--actor <name>]
             [--expect-revision <n>] [--format json|plain]
pin parent <id|id-prefix|filename> <parent-id> [--actor <name>]
             [--expect-revision <n>] [--format json|plain]
pin relate <id|id-prefix|filename> <related-id> [--actor <name>]
             [--expect-revision <n>] [--format json|plain]
pin doctor [--repair] [--upgrade] [--strict] [--format json|plain]
pin archive <id|prefix|filename>
            [--resolution implemented|rejected|superseded|stale]
            [--note <text>] [--format json|plain]
pin unarchive <id|prefix|filename> [--format json|plain]
pin read <id|prefix|filename> [--format json|plain]
pin edit <id|prefix|filename> [--format json|plain]
pin rm <id|prefix|filename> [--format json|plain]
pin import <directory> [--force] [--format json|plain]
pin export <directory> [--force] [--format json|plain]
pin stats [--format json|plain]
pin view [--project <name>] [--tag <name>] [--kind <kind>] [--type <type>] [--status <status>] [--archived|--all] [--port <n>] [--no-open] [--format json|plain]
pin view-project [--tag <name>] [--kind <kind>] [--type <type>] [--status <status>] [--archived|--all] [--port <n>] [--no-open] [--format json|plain]
pin --help
pin --version
```

`add` requires one primary domain and rejects duplicate titles within a project by default. Use `--allow-duplicate` when the repetition is intentional. New records start as `captured` work items.

## Work lifecycle

Work states are `captured`, `planned`, `in_progress`, `blocked`, `review`, `done`, `closed`, and `cancelled`. `captured` means an idea has not been planned yet. `done` requires completion evidence. `closed` records acceptance and is separate from archive visibility.

`pin next` returns planned work whose dependencies are complete and whose claim is available. `pin claim` moves planned work to `in_progress`. Releasing an in-progress claim returns it to `planned` so another agent can pick it up.

Each mutation records an activity event in the Markdown file. The file also stores the current handoff, claim lease, revision, and dependency links.

## Proposal domains

Every new pin has one primary `kind`:

- `technical` — architecture, reliability, security, performance, infrastructure, and developer experience
- `product` — user problems, workflows, features, usability, onboarding, and accessibility
- `business` — adoption, positioning, distribution, monetization, partnerships, and value capture
- `project` — maintenance, releases, documentation, community, contribution process, and governance

Cross-cutting proposals use the kind matching their primary intended outcome and ordinary tags for secondary concerns. Legacy files without a kind remain compatible and appear as `unspecified`.

Filter or group by domain:

```bash
pin list-project --kind technical --format table
pin context --group kind --format plain
```

## Project identity

Without `--project`, `pin` resolves the project in this order:

1. `PIN_PROJECT`
2. `.pin-project` at the Git repository root
3. the Git repository directory name
4. the current directory name outside a repository

This keeps ideas associated with the same project when commands run from nested directories.

## Global, local, and team vaults

The default vault is `~/.pin_vault`. Override it with `PIN_VAULT`.

To create a project-local vault:

```bash
pin init --local --project my-project
git add .pin-project .pin_vault
```

When `.pin_vault` exists at the repository root, `pin` discovers it automatically. Commit that directory to share and synchronize proposals through Git. `PIN_VAULT` always takes precedence.

Use `pin export <directory>` and `pin import <directory>` for backups or moving Markdown proposals between vaults. Existing filenames are skipped unless `--force` is supplied.

## Integrity and lifecycle

Inspect a vault without changing it:

```bash
pin doctor --format plain
```

`doctor` reports malformed or unreadable files, invalid metadata, duplicate IDs, and legacy fields. It exits non-zero for integrity errors; `--strict` also treats warnings as failures. `--repair` performs conservative repairs such as adding schema/ID metadata. `--upgrade` converts readable legacy records to schema 2 and adds work-item defaults.

Archive completed or rejected proposals without destroying their history:

```bash
pin archive a82f71 --resolution implemented --note "Shipped in v1.3.0"
pin list-project --archived
pin unarchive a82f71
```

Archived proposals are excluded from ordinary list, search, and context output. Use `--archived` for archived-only output or `--all` for both states. `rm` remains permanent deletion.

`pin edit` validates front matter after the editor exits. An invalid edit is saved to a recovery file, while the last valid proposal is restored.

## Search behavior

Search uses deterministic multi-term AND matching. Title matches rank above tag matches, which rank above body-only matches; priority and recency break ties. Use `--limit` to bound agent output. JSON search records include an additive `score` field and work-item state.

## Output contract

- Interactive `list` and `search` output defaults to a table.
- Interactive mutations default to concise plain text.
- Redirected or piped output defaults to JSON.
- `--format` makes output deterministic for scripts and agents.
- Diagnostics go to stderr and failures return a non-zero exit status.
- JSON tags are arrays and every record includes its primary `kind`.

## Storage compatibility

New work items use schema version `2` and a 12-character stable ID as both metadata and filename. Older timestamp-named Markdown files and metadata without `schema` remain readable, receive a deterministic derived ID when needed, and appear as captured ideas until upgraded. Archive state is stored as ordinary `archived_at`, `resolution`, and `resolution_note` front-matter fields. The vault remains plain Markdown and does not require a database.
