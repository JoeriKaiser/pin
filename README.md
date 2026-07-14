# pin

A zero-runtime-dependency idea registry for humans and coding agents.

`pin` saves forward-looking proposals and implementation roadmaps as Markdown with YAML front matter. It is intentionally smaller than a task tracker: capture what should improve, find it later, and give the next agent a compact project context.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/install.sh | sh
```

Supported Linux and macOS platforms use release binaries. The source fallback requires Zig 0.16.0.

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

At session start, agents can request compact proposals grouped by domain:

```bash
pin context --limit 10 --group kind --format plain
```

The bundled skill also defines a proactive curation protocol: agents notice substantial out-of-scope improvements during normal work, apply strict evidence and quality gates, deduplicate them, and add at most one pin per ordinary session.

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
```

## Commands

```text
pin init --local [--project <name>] [--format json|plain]
pin add <markdown> --kind technical|product|business|project
                     [--stdin] [--project <name>] [--title <title>]
                     [--tags <csv>] [--priority low|medium|high]
                     [--allow-duplicate] [--format json|plain]
pin list [--project <name>] [--tag <name>] [--kind <kind>]
         [--format json|table|plain]
pin list-project [--tag <name>] [--kind <kind>] [--format json|table|plain]
pin search <query> [--project <name>] [--tag <name>] [--kind <kind>]
                   [--format json|table|plain]
pin context [--project <name>] [--kind <kind>] [--limit <n>]
            [--group kind] [--format json|plain]
pin read <id|prefix|filename> [--format json|plain]
pin edit <id|prefix|filename> [--format json|plain]
pin rm <id|prefix|filename> [--format json|plain]
pin import <directory> [--force] [--format json|plain]
pin export <directory> [--force] [--format json|plain]
pin stats [--format json|plain]
pin --help
pin --version
```

`add` requires one primary domain and rejects duplicate titles within a project by default. Use `--allow-duplicate` when the repetition is intentional.

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

## Output contract

- Interactive `list` and `search` output defaults to a table.
- Interactive mutations default to concise plain text.
- Redirected or piped output defaults to JSON.
- `--format` makes output deterministic for scripts and agents.
- Diagnostics go to stderr and failures return a non-zero exit status.
- JSON tags are arrays and every record includes its primary `kind`.

## Storage compatibility

New ideas use a 12-character stable ID as both metadata and filename. Older timestamp-named Markdown files remain readable and receive a deterministic derived ID when listed. The vault remains plain Markdown and does not require a database.
