# Agent Mail

Agent Mail is mail-style coordination for Codex main threads and subagents.
It gives agents a small shared surface for three ordinary actions:

```text
see the team
write a note
read visible context
```

Use it when several Codex agents are working in the same conversation tree and
need a clear, low-friction way to address each other.

## Current Status

Agent Mail has two pieces:

- The installable Codex plugin and `agent-mail` CLI in this repo.
- Hosted `agent_mail.*` support in Codex core for real GUI-native mail.

The plugin and CLI exist today. The hosted mail transport is still landing in
Codex, so the CLI is intentionally honest:

- `agent-mail team` shows the current main/subagent family.
- `agent-mail write` resolves a target, then requires host-native
  `agent_mail.write` before it claims delivery.
- `agent-mail read` resolves a target, then requires host-native
  `agent_mail.read` before it claims transcript access.

There is no local mailbox fallback and no separate Agent Mail server to start.
The plugin hooks invoke the packaged Rust CLI; Codex core owns real delivery.

## Install

Install the plugin marketplace source:

```bash
codex plugin marketplace add pauljunsukhan/codex-power-utils
```

Then in Codex:

```text
1. Open Plugins.
2. Enable Agent Mail.
3. Open Hooks review, or run /hooks in the CLI/TUI.
4. Trust the Agent Mail hooks.
5. Start a new Codex session.
```

Hooks only add a short model-visible reminder that Agent Mail exists. They do
not deliver mail, prove reachability, or replace hosted `agent_mail.*` tools.

## Quick Start

List the current team:

```bash
agent-mail team
```

Use `--technical` when a name is ambiguous:

```bash
agent-mail team --technical
```

When host-native mail is available, send a queued note:

```bash
agent-mail write subagent:1 "Can you check the failing test?"
```

Ask for immediate interruption only when the recipient should change course:

```bash
agent-mail write subagent:1 "Pause and read the latest patch first." --interrupt
```

Read recent visible context from a reachable target:

```bash
agent-mail read subagent:1 --limit 10
```

If `write` or `read` fails with a host-native-required message, the note was not
delivered. Update Codex when native `agent_mail.write/read` support is available,
then rerun the command.

## Team Directory

`team` shows only the current main/subagent family and the handles that Agent
Mail can resolve:

```text
Team

handle         name                         state
main           Build the docs               main
subagent:1     Dirac                        open
subagent:2     Noether                      open
```

These aliases are equivalent:

```bash
agent-mail coworkers
agent-mail contacts
agent-mail subagents
agent-mail ls
```

The directory is for addressing. It is not delivery proof, and it is not a
search engine for every historical Codex thread.

## Addresses

Use stable, human-readable targets:

```text
main
subagent
subagent:N
Dirac
Dirac#2
role:reviewer
Dirac:tester
```

Rules:

- `main` is the main thread for this family.
- `subagent` works only when there is exactly one subagent.
- `subagent:N` is stable within the current family.
- A bare name works only when it is unique.
- Repeated names get suffixes such as `Dirac#1` and `Dirac#2`.
- Role aliases work only when they are unique.

Use `agent-mail team --technical` when Agent Mail asks you to disambiguate.

## Writing Mail

Hosted Agent Mail writes queued GUI mail by default:

```text
deliveryMode = "queue"
forwardNext = 3
requireReply = false
```

Queue mode means the note should appear as native GUI mail without interrupting
or starting the recipient's current work.

Interrupt delivery is explicit:

```text
deliveryMode = "interrupt"
```

`forwardNext` exists because agents often react to the first note before the
sender's immediate follow-up context arrives. By default, Agent Mail forwards
the sender's next three visible messages as one labeled follow-up context item.

`requireReply` is for explicit delegation. Ordinary coordination should not
block the sender.

## Reading Context

`agent_mail.read` browses a reachable agent's live visible transcript. It is not
an inbox check; mail arrives through the GUI.

Default:

```text
latest 10 visible transcript messages
```

Examples:

```text
agent_mail.read({ target: "subagent:1" })
agent_mail.read({ target: "main", limit: 25 })
agent_mail.read({
  target: "subagent:1",
  since: "2026-06-05T22:00:00Z",
  until: "2026-06-05T22:30:00Z"
})
```

The CLI bridge mirrors the same shape:

```bash
agent-mail read subagent:1 --limit 25
```

## Delivery Receipts

A successful write should give the sender a receipt that answers the practical
question agents care about: did Codex resolve the target and place this note in
the recipient's native mail surface?

If native support is unavailable, Agent Mail fails clearly rather than
pretending the message landed. A local file, stale transcript snapshot, or
terminal scrape is not a delivery receipt.

## Troubleshooting

Check whether the CLI is available:

```bash
command -v agent-mail
agent-mail team
```

If hooks do not seem active:

```text
1. Re-open Hooks review, or run /hooks.
2. Trust Agent Mail hooks.
3. Start a new session.
```

If the packaged plugin cannot find a binary for your platform, rebuild the local
cartridge:

```bash
scripts/package-agent-mail-plugin.sh
scripts/smoke-test-agent-mail-plugin.sh
```

## Local Development

For a local checkout, package the binary and add this repo as a marketplace
source:

```bash
scripts/package-agent-mail-plugin.sh
scripts/smoke-test-agent-mail-plugin.sh
codex plugin marketplace add /Users/paulhan/dev/codex-power-utils
```

The local Codex app link is:

```text
codex://plugins/agent-mail?marketplacePath=/Users/paulhan/dev/codex-power-utils/.agents/plugins/marketplace.json
```

Build and test the Rust CLI:

```bash
cargo test -p agent-mail
cargo clippy -p agent-mail --all-targets -- -D warnings
```

For the host implementation contract, see
[agent-mail-implementation-spec.md](agent-mail-implementation-spec.md).
