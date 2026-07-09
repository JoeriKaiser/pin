---
name: pin
description: Saves, filters, and searches novel project ideas and recommended next steps inside ~/.pin_vault/
---

# Using the Pin Tool (Project Improvement Registry)

The `pin` CLI tool manages **novel ideas about a project** and **recommended next steps towards improving a project** inside a unified vault located at `~/.pin_vault/` (configurable via `PIN_VAULT`).

This tool is **not** a general knowledge system, wiki, code documentation repository, or task-status tracker. Its sole purpose is to act as a registry of forward-looking proposals and concrete execution roadmaps for project improvement.

---

## 1. Scope & Threshold (What to Pin)

### What to Save:
- **Novel Project Ideas**: Creative suggestions, new features, structural refactoring concepts, or design improvements.
- **Recommended Next Steps**: Actionable, step-by-step roadmaps or checklists detailing how to realize an improvement or execute a new idea.
- **Improvement Proposals**: Clear rationale for why a specific part of the system should be optimized, replaced, or redesigned.

### What is Strictly Forbidden:
- **General Documentation / Wiki Entries**: Explanations of how the current code works, API guides, database schemas, or dependency lists.
- **Trivial Code Checkpoints**: Ephemeral backups of unfinished work, code drafts, or basic git-like logs.
- **Task Status / Todo Updates**: Status tracking of standard developer tasks.
- **Raw Compiler / Tool Logs**: Plain stdout/stderr outputs or error logs.

---

## 2. Markdown Template for Ideas

When writing a new idea using `pin add`, format the markdown content to focus on the proposal and next steps:

```markdown
# [Title of the Novel Idea]

## Idea & Proposal
[Detailed explanation of the novel suggestion or improvement]

## Why it Improves the Project
[The expected benefits, impact on performance/maintainability, or problems solved]

## Recommended Next Steps
[A concrete, step-by-step checklist of actions needed to realize this idea]
```

### Title Guidelines
- Keep titles as short noun phrases, ideally under 60 characters.
- Use descriptive nouns, not full sentences (e.g. "Lazy widget loading" not "We should implement lazy loading for widgets").

---

## 3. Tool Capabilities

### `pin add "<markdown_content>" [--project <name>] [--title <string>] [--tags <comma,separated>]`
- **Purpose**: Commits a new novel idea and recommended next steps to the vault.
- **Parameters**:
  - `--project`: Associate the idea with a project (defaults to current directory name).
  - `--title`: A descriptive short noun phrase for listing (auto-extracted from content if omitted, max 60 chars).
  - `--tags`: Comma-separated labels for categorization (e.g. `perf, ux, refactor`).
- **Stdin**: Use `--stdin` flag or pipe content directly: `cat notes.md | pin add --title "My Idea"`.

### `pin list [--project <name>] [--tag <name>] [--format table]`
- **Purpose**: Retrieves a JSON index of saved ideas, sorted newest first.
- **Flags**: `--project` filters by project, `--tag` filters by tag, `--format table` for human-readable output.
- **Backward compat**: `pin list-project` still works (equivalent to `pin list --project <cwd>`).

### `pin search "<query>" [--tag <name>] [--format table]`
- **Purpose**: Case-insensitively searches all saved ideas and next steps inside the vault.

### `pin read <filename>`
- **Purpose**: Outputs the full proposal and next steps of the selected idea.

### `pin rm <filename>`
- **Purpose**: Removes an idea from the vault.

### `pin edit <filename>`
- **Purpose**: Opens an idea in your `$EDITOR` for refinement.

### `pin stats`
- **Purpose**: Displays vault summary: total ideas, projects, tags, date range.

### Environment
- `PIN_VAULT`: Override default vault path (`~/.pin_vault`).

---

## 4. Agent Execution Protocols

### Phase A: Discovering Ideas (Read-First)
1. **Auto-scan on session start**: At the beginning of every session, run `pin list-project` (or `pin list --project <name>`). If any pins exist, mention them in your first response.
2. **Respect Proposed Roadmaps**: Read the full file content using `pin read <filename>` before implementing an improvement to align with the recommended execution steps.

### Phase B: Documenting New Ideas (Write-Second)
1. **Register Novel Concepts**: If you conceive of a novel feature, design improvement, or major refactor during development, save it using `pin add` with the recommended next steps. Use `--tags` for categorization.
2. **Formulate Next Steps**: Before finishing a session, if there are remaining steps to fully realize an improvement, document them as a pin so the user or the next agent can seamlessly resume the work.
