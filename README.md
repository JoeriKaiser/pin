# pin

zero deps idea CLI tool for you and your agents

A lightweight vault for saving ideas and discoveries that should be implemented but are out of scope right now. Both humans and AI agents can use it to pin forward-looking proposals and next-step roadmaps.

Ideas are stored as markdown files with YAML front matter in `~/.pin_vault/` (override with `PIN_VAULT` env var).

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/install.sh | sh
```

Requires [Zig](https://ziglang.org) compiler.

## Agent Integration

```bash
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/SKILL.md -o ~/.agent_skills/pin.md
```

## Usage

```bash
# Add an idea
pin add "markdown content" [--project <name>] [--title <string>] [--tags <comma,separated>]

# Add from stdin
cat notes.md | pin add --stdin --title "My Idea" --project myproj

# List all ideas (JSON, newest first)
pin list

# List with filters and human-readable output
pin list --project <name> --tag <name> --format table

# List ideas for the current project folder
pin list-project

# Search all ideas (case-insensitive)
pin search "<query>" [--tag <name>] [--format table]

# Read a specific idea
pin read <filename>

# Remove an idea
pin rm <filename>

# Edit an idea in $EDITOR
pin edit <filename>

# Vault statistics
pin stats
```

## Output Formats

- **JSON** (default): Machine-readable, sorted newest first. Includes `filename`, `project`, `title`, `timestamp`, and optional `tags`.
- **Table** (`--format table`): Human-readable with date, project, title, tags, and filename.
