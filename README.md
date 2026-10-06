# pin

A local-first, Git-backed personal issue tracker and work ledger for coding agents and orchestrating humans.

`pin` saves tasks, bugs, ideas, and architectural decisions as Markdown files with YAML front matter. Records follow a clear lifecycle from creation to verified completion. Agents claim work, coordinate handoffs, enforce dependency ordering, and record verification evidence directly from the terminal or tool calls. Human orchestrators inspect, steer, and update work in real time using the built-in local web viewer.

The vault is plain Markdown in `.pin_vault/`, tracked in Git, and requires zero external databases, cloud services, or network requests.

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

The installer detects AMD64 or ARM64, verifies the release checksum, installs to `%LOCALAPPDATA%\Programs\pin`, and adds that directory to the user PATH.

## Agent integration

```bash
mkdir -p ~/.agents/skills/pin
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/SKILL.md \
  -o ~/.agents/skills/pin/SKILL.md
```

### Agent workflow loop

Agents query planned work, acquire atomic claim leases, record progress, and complete items with verifiable evidence:

```bash
# 1. Inspect active project context and retrieve the next ready work item
pin context --limit 10 --format plain
pin next --limit 1 --format json

# 2. Claim work before modifying the repository (default 1-hour lease)
pin claim <id> --actor agent:name --lease 3600

# 3. Leave structured handoff notes across turns or subagents
pin handoff <id> --actor agent:name --progress "Implemented core parser" --next "Add edge-case tests"

# 4. If blocked, mark status with an explicit blocker reason
pin transition <id> --to blocked --actor agent:name --note "Waiting on upstream schema fix"

# 5. Complete work with non-empty verification evidence
pin complete <id> --actor agent:name --evidence "cargo test passes (17/17 tests green)"

# 6. Human orchestrator or lead agent closes the item
pin close <id> --actor human:name
```

Set `PIN_ACTOR` to provide the default actor identity for CLI mutations. The web viewer records `human:viewer` by default, configurable with `PIN_VIEWER_ACTOR`.

## Quick start

```bash
# Add a task, bug, or idea
pin add '# Parser edge-case handling

Handle unbalanced brackets gracefully.' --type bug --priority high

# Query next ready item
id=$(pin next --format json | jq -r '.[0].id')

# Claim, execute, and complete
pin claim "$id" --actor agent:gemini
pin complete "$id" --actor agent:gemini --evidence "All unit tests pass"

# Launch the live orchestrator viewer
pin view --no-open --format plain
```

## Commands

```text
pin init --local [--project <name>] [--format json|plain]
pin add <markdown> [--kind technical|product|business|project]
                     [--type task|bug|idea|decision] [--status <status>]
                     [--actor <name>] [--stdin] [--project <name>]
                     [--title <title>] [--tags <csv>] [--priority low|medium|high]
                     [--allow-duplicate] [--format json|plain]
pin list [--project <name>] [--tag <name>] [--kind <kind>]
          [--type <type>] [--status <status>] [--claimed-by <actor>]
          [--ready] [--archived|--all] [--format json|table|plain]
pin list-project [--tag <name>] [--kind <kind>] [--type <type>]
                  [--status <status>] [--claimed-by <actor>]
                  [--ready] [--archived|--all] [--format json|table|plain]
pin search <query> [--project <name>] [--tag <name>] [--kind <kind>]
                    [--type <type>] [--status <status>] [--claimed-by <actor>]
                    [--limit <n>] [--archived|--all] [--format json|table|plain]
pin context [--project <name>] [--kind <kind>] [--type <type>]
             [--status <status>] [--limit <n>]
             [--group kind] [--archived|--all] [--format json|plain]
pin next [--project <name>] [--limit <n>] [--format json|plain]
pin transition <id|prefix|filename> --to <status> [--actor <name>]
             [--note <text>] [--expect-revision <n>] [--format json|plain]
pin claim <id|prefix|filename> [--actor <name>] [--lease <seconds>]
             [--expect-revision <n>] [--format json|plain]
pin release <id|prefix|filename> [--actor <name>] [--force]
             [--expect-revision <n>] [--format json|plain]
pin handoff <id|prefix|filename> [--actor <name>] [--progress <text>]
             [--next <text>] [--blocker <text>] [--verification <text>]
             [--expect-revision <n>] [--format json|plain]
pin complete <id|prefix|filename> --evidence <text> [--actor <name>]
             [--expect-revision <n>] [--format json|plain]
pin close <id|prefix|filename> [--actor <name>] [--note <text>]
             [--expect-revision <n>] [--format json|plain]
pin depend <id|prefix|filename> <dependency-id> [--actor <name>]
             [--expect-revision <n>] [--format json|plain]
pin parent <id|prefix|filename> <parent-id> [--actor <name>]
             [--expect-revision <n>] [--format json|plain]
pin relate <id|prefix|filename> <related-id> [--actor <name>]
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
pin view [--project <name>] [--tag <name>] [--kind <kind>] [--type <type>]
          [--status <status>] [--archived|--all] [--port <n>] [--no-open] [--format json|plain]
pin view-project [--tag <name>] [--kind <kind>] [--type <type>] [--status <status>]
                  [--archived|--all] [--port <n>] [--no-open] [--format json|plain]
pin --help
pin --version
```

## Work lifecycle

Records move through explicit lifecycle states:

- `created` — originated idea, task, or bug; not yet planned or triaged.
- `planned` — triaged, prioritized, unblocked, ready for agent execution.
- `in_progress` — actively claimed by an agent or human with a valid lease.
- `blocked` — work halted; requires blocker resolution or dependency completion.
- `review` — implementation finished; awaiting human or reviewer-agent verification.
- `done` — verified completed with concrete evidence.
- `closed` — accepted outcome; terminal state.
- `cancelled` — abandoned or rejected; terminal state.

### Dependency gating

`pin depend <item> <dependency>` records directed dependencies. `pin next` evaluates dependency readiness: an item is only returned when all of its dependencies are `done` or `closed`. Cycles are rejected during dependency declaration.

### Concurrency and locking

Every item record includes an integer `revision`. Parallel agents can pass `--expect-revision <n>` to detect concurrent modifications. Mutations acquire an atomic file lock (`.<id>.md.lock`) with automatic retry and stale-lock eviction to guarantee integrity during multi-agent execution.

## Human orchestrator viewer (`pin view`)

Running `pin view` launches an embedded HTTP server serving a single-page management interface:

- Live updates: automatically refreshes vault changes made by background agents.
- Status, type, and priority filtering.
- Full markdown rendering and activity history timeline.
- Interactive steering: buttons to move items to planned, claim, mark blocked, complete with evidence, or close.
- CSRF-protected mutation API (`POST /{token}/items/{id}/action`).

## Storage compatibility

Items use schema version `2` with 12-character hex IDs. Legacy proposals remain readable and can be migrated using `pin doctor --upgrade`.
