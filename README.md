# Codex Power Utils

Small command-line utilities for working with Codex agents as a practical
engineering control plane.

The first planned utility is `agent-mail`: a simple way to discover the current
agent family, send live messages to coagents, and read/watch their visible
conversation history.

## Utilities

- `agent-mail`: mail-style coordination between parent, side, and child agents.

## Design Principles

- Keep commands small and memorable.
- Prefer live agent messages over file handoffs.
- Make GUI-visible steering explicit when needed.
- Scope default discovery to the current agent family.
- Allow wider cross-session search without making every command scan all history.
- Treat a message as delivered only when transcript history shows it landed or
  behavior changed.

## Current Status

Specification first. Implementation and Codex app integration are next.

See [docs/agent-mail.md](docs/agent-mail.md).
