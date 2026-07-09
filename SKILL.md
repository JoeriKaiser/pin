---
name: pin
description: Saves, filters, and retrieves deep markdown-based ideas inside ~/.pin_vault/
---

# Using the Pin Tool

The `pin` CLI tool manages deep markdown-based ideas inside `~/.pin_vault/`. Use it to save local checkpoints of code designs, planning notes, and architectural discussions in project folders.

## Capabilities

1. **add**: Add a new markdown idea.
   `pin add "<detailed_markdown_content>" [--project <name>] [--title <string>]`
   - Generates unique filename: `YYYY-MM-DD_UNIXTIMESTAMP.md`
   - Automatically uses the current directory's base name as the project if `--project` is omitted.
   - Automatically uses the first 30 characters of the content as the title if `--title` is omitted.

2. **list**: List all ideas from the vault in a fast JSON array.
   `pin list`

3. **list-project**: List ideas filtered strictly to the current project.
   `pin list-project`

4. **search**: Case-insensitively search the entire file content (front matter + body) for a query string.
   `pin search "<query>"`

5. **read**: Read the raw content (front matter + body) of a specific idea.
   `pin read <filename>`

## Usage Instructions for Agents

- **Context Retrieval**: When starting in a codebase, run `pin list-project` to see if there are any existing architectural notes, checklists, or designs saved for this directory.
- **Reading Notes**: Use `pin read <filename>` to recover the full markdown text of an idea listed in the JSON summary.
- **Searching Ideas**: Use `pin search "<query>"` to locate previous designs or notes across all projects using keywords.
- **Documenting Decisions**: Use `pin add` whenever you make load-bearing architectural changes, design plans, or save brainstorm notes. Do not hesitate to use it to document complex technical contexts.
