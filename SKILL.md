---
name: pin
description: Proactively curates substantial project-specific improvement proposals in a Markdown vault. Use at the start and end of every software project session, and whenever work reveals a meaningful out-of-scope technical, product, business, or project improvement. Load existing context first; capture justified, distinct, actionable ideas sparingly.
---

# Using Pin

`pin` is a durable registry of **future project improvements** and **recommended execution roadmaps**. Treat improvement capture as a lightweight background responsibility during project work.

Act as a curator, not an idea generator: notice continuously, evaluate strictly, and write sparingly. Never manufacture a pin merely to satisfy this protocol.

Pins live as Markdown with YAML front matter in `~/.pin_vault`, a repository-local `.pin_vault`, or `PIN_VAULT`.

## Session protocol

### 1. Load context first

At the beginning of every project session, run:

```bash
pin context --limit 10 --group kind --format plain
```

Mention relevant existing pins briefly. Context provides awareness and prevents duplication; it is not a mandate to implement or elaborate on every proposal.

Before implementing an existing proposal, read it completely:

```bash
pin read <id-or-prefix>
```

### 2. Notice candidates during normal work

Use two observation lenses without interrupting the current task to brainstorm:

1. **Technical lens:** Did the code, runtime, architecture, security posture, or developer workflow reveal a substantial improvement?
2. **Product/business/project lens:** Did the work reveal a meaningful opportunity in user value, adoption, delivery, maintenance, community, or long-term project health?

The lenses ensure consideration, not quotas. Do not create one pin per category.

### 3. Apply the curation gate

Create a pin only when every condition is met:

- **Project-specific:** grounded in this project's goals, code, users, constraints, or domain.
- **Substantial:** materially improves value, reliability, security, performance, maintainability, adoption, or sustainability.
- **Justified:** supported by observed evidence or a clearly labeled hypothesis.
- **Out of scope:** not something that should simply be completed within the current task.
- **Distinct:** not already represented by an existing pin.
- **Actionable:** includes a plausible validation or implementation roadmap.

Best-practice knowledge qualifies only when there is concrete evidence of a relevant project gap. “Projects should have tests” is not enough; identify the actual untested risk and consequence.

Urgent defects and issues within the current task must be surfaced or fixed directly, not buried in the vault.

### 4. Search before writing

Search by the candidate's important terms:

```bash
pin search "<key terms>" --project <project> --format plain
```

If an existing pin covers the same outcome, do not create another. Read or refine the existing proposal when appropriate.

### 5. Curate at session end

Evaluate substantial candidates before finishing. In a normal session, automatically add at most **one** new pin. More are appropriate only during an explicit audit, review, or ideation task.

Do not ask for permission for a qualifying pin; add it and mention it briefly in the final response:

> Pinned **Incremental configuration validation** (`a82f71`).

If no candidate passes the gate, create nothing.

## Pin domains

Every new pin has exactly one primary `kind`. Choose by its intended outcome; use tags for secondary concerns.

### `technical`

How the system is built and operated: architecture, code quality, performance, reliability, security, infrastructure, and developer experience.

Require concrete technical evidence, an engineering consequence, and a plausible implementation path.

### `product`

What users experience and value: problems, workflows, features, usability, onboarding, and accessibility.

Identify the user or workflow, expected value, evidence and assumptions, and a way to validate the need.

### `business`

How the project reaches users or sustains value: adoption, positioning, distribution, monetization, partnerships, and value capture.

Agents usually lack direct market evidence. State hypotheses as hypotheses, identify the likely audience and value mechanism, and include a low-cost validation experiment. Never present general market knowledge as project-specific fact.

### `project`

How the project is maintained and delivered: releases, documentation, community, contribution process, governance, and operational sustainability.

Identify recurring project friction and a sustainable process improvement.

For cross-cutting ideas, choose the kind matching the primary intended outcome. For example, a shared vault is `product` when the goal is collaboration, but `business` when the proposal is specifically a paid team offering.

Legacy pins without a kind appear as `unspecified`.

## Proposal template

Use short noun-phrase titles and this structure:

```markdown
# Proposal title

## Idea & Proposal
Describe the concrete improvement or hypothesis.

## Evidence
- Reference relevant files, observed behavior, user workflow, constraints, or domain facts.

## Why it Improves the Project
Explain the expected project-specific impact.

## Confidence & Assumptions
Confidence: high | medium | low

- State important assumptions and uncertainty.
- Product and business pins must include a low-cost validation method.

## Recommended Next Steps
- [ ] Give concrete validation or implementation steps.
```

Classify the proposal's origin with a tag when useful:

- `observed` — directly supported by project evidence
- `best-practice` — a concrete project gap identified using domain practice
- `exploratory` — a plausible but unvalidated novel direction

Add it with an explicit domain:

```bash
pin add --stdin --kind technical --title "Short title" \
  --tags "observed,reliability" --priority high <<'EOF'
# Short title

## Idea & Proposal
...

## Evidence
- `src/example.zig` silently ignores malformed records.

## Why it Improves the Project
...

## Confidence & Assumptions
Confidence: high

## Recommended Next Steps
- [ ] ...
EOF
```

Duplicate titles are rejected by default. Use `--allow-duplicate` only when the outcomes are genuinely distinct.

## Do not pin

- generic best-practice checklists without a demonstrated gap
- routine status updates, ordinary todos, or current-task work
- general documentation about how existing code works
- raw compiler output, logs, or unfinished code snapshots
- trivial cleanup or speculative technology substitutions
- urgent risks that should be disclosed immediately
- variations of an existing proposal

## Commands

- `pin context [--project <name>] [--kind <kind>] [--limit <n>] [--group kind] [--format json|plain]`
- `pin add <markdown> --kind technical|product|business|project [--stdin] [--project <name>] [--title <title>] [--tags <csv>] [--priority low|medium|high] [--format json|plain]`
- `pin list [--project <name>] [--tag <name>] [--kind <kind>] [--format json|table|plain]`
- `pin list-project [--tag <name>] [--kind <kind>] [--format json|table|plain]`
- `pin search <query> [--project <name>] [--tag <name>] [--kind <kind>] [--format json|table|plain]`
- `pin read|edit|rm <id|prefix|filename>`
- `pin stats [--format json|plain]`
- `pin init --local [--project <name>]`
- `pin import|export <directory> [--force]`

Project identity resolves from `--project`, `PIN_PROJECT`, repository-root `.pin-project`, repository name, then current directory name. Vault location resolves from `PIN_VAULT`, repository-root `.pin_vault`, then `~/.pin_vault`.
