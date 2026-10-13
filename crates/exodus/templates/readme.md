# .exodus

Written by `exodus` (https://github.com/elim-io/exodus) for this repository. `exodus init` adds the folder to
`.gitignore`: it holds this machine's work, not the project's source.

- `project.json`: the name, the kind (move or new) and, once an account is linked, the project.
- `findings.json`: what `exodus audit` read (for a new product, what the agent wrote). Findings, never source code.
- `answers.json`: the answers to the audit's questions, and who found each.
- `report.json`, `report.md`: the report, built here by `exodus report`, or written by `exodus send` and `exodus report --pull` from the platform.
- `plan.md`: the tasks of the move in order; `exodus task done` ticks them.
- `audit.log`, `cache/`: what the last audit printed, and knowledge base articles read.

The agent's instructions are outside this folder, so they can be committed: `.claude/skills/exodus/SKILL.md`, the
Exodus section of `AGENTS.md`, `.cursor/rules/exodus.mdc`.
