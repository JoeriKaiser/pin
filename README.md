# pin

A lightweight, zero-dependency, hyper-fast CLI tool written in Zig for saving, filtering, and retrieving deep markdown-based ideas into a unified vault directory located at `~/.pin_vault/`.

## Features

- **Zero Dependencies**: Pure, modern Zig 0.16.0 standard library implementations.
- **Sub-millisecond Performance**: Stream-based JSON parsing and file iteration that reads only front-matter metadata without loading entire markdown bodies into memory.
- **O(1) Memory JSON Output**: Directly formats and streams JSON arrays to `stdout`.
- **Automatic Context Mapping**: Defaults to using the current directory base name as the project context and auto-generates clean, UTF-8-safe titles if omitted.

## Installation

To download and install the binary globally on your system, run:

```bash
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/install.sh | sh
```

### Manual Compilation

If you have Zig installed locally, you can clone the repository and compile the binary directly:

```bash
zig build-exe main.zig -O ReleaseSafe
mv main /usr/local/bin/pin
```

---

## Agent Integration (Skills)

For AI agents (like Claude, ChatGPT, or other LLMs operating locally), you can install the `pin` skill to enable structured repository planning and brainstorming retrieval:

```bash
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/SKILL.md -o ~/.agent_skills/pin.md
```

Once installed, the agent will dynamically look up folder context and manage system notes using this tool.

---

## Usage Command Reference

### 1. Add an Idea
Saves a markdown-based idea to the vault.
```bash
pin add "This is my detailed design idea." --project "MyProject" --title "Custom Title"
```
- **Fallback Behavior**:
  - If `--project` is omitted, it defaults to the folder name of your current working directory.
  - If `--title` is omitted, it defaults to the first 30 characters of the content.

### 2. List All Ideas
Streams back a fast JSON summary array of all files in the vault.
```bash
pin list
```
*Output*: `[{"filename":"2026-07-09_1783632296.md","project":"MyProject","title":"Custom Title","timestamp":1783632296}]`

### 3. List Project Ideas
Streams back a filtered JSON summary array of ideas corresponding exactly to the current project (base name of current directory).
```bash
pin list-project
```

### 4. Search Ideas
Case-insensitively searches the entire file content (front matter + body) for a query string, streaming back matching ideas.
```bash
pin search "<query>"
```

### 5. Read an Idea
Outputs the absolute raw content (Front matter + Markdown body) of a specific idea file in the vault.
```bash
pin read <filename>
```
