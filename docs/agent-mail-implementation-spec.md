# Agent Mail Implementation Spec

This is the strict implementation contract for Agent Mail inside Codex.

The public product name is **Agent Mail**. Internal version labels should stay
out of customer-facing copy.

## Objective

Agent Mail is a host-native mail surface for Codex main threads and subagents.
It must use Codex runtime identity, agent graph, inter-agent communication,
mailbox delivery, GUI items, and transcript access. The plugin and CLI are only
install/debug/bridge surfaces.

## Hosted Surface

Hosted tools:

```text
agent_mail.team
agent_mail.write
agent_mail.read
agent_mail/capabilities
```

Out of scope for this cut:

```text
agent_mail.wait
agent_mail.agents
agent_mail.find
sync_pulse
```

`sync_pulse` is intentionally paused. Do not reserve implementation behavior for
it in this pass.

## Native Codex Substrate

Agent Mail must layer on the existing Codex delivery pieces:

```text
Agent Mail envelope and receipt state
  -> InterAgentCommunication
  -> recipient InputQueue mailbox
  -> app-server ThreadItem / GUI notification
```

Responsibilities:

- `InterAgentCommunication`: native author/recipient/content envelope.
- `InputQueue`: recipient-side staging, drain, and `trigger_turn` behavior.
- `ThreadItem` / app-server notifications: visible GUI/history object.
- Agent Mail state: message id, receipt, read/unread, forwarding, and reply
  metadata.

The mailbox queue alone is not a GUI mail item. A GUI item alone is not delivery
to the recipient agent. Real Agent Mail needs both.

## Required Host Modules

Recommended areas:

```text
codex-rs/core/src/agent_mail/
codex-rs/core/src/tools/handlers/agent_mail.rs
codex-rs/state/src/model/agent_mail.rs
codex-rs/app-server-protocol/src/protocol/v2/agent_mail.rs
codex-rs/app-server-protocol/src/protocol/v2/item.rs
codex-rs/app-server/src/agent_mail.rs
```

Do not shell out from Codex core to the `agent-mail` CLI.

## Team

Schema:

```ts
agent_mail.team(args?: {
  includeClosed?: boolean
}): AgentMailTeam
```

Default output is the current live main/subagent family only.

Canonical handles:

```text
main
subagent
subagent:N
```

Accepted friendly identifiers for `write` and `read`:

```text
unique visible name
Name#N
role:<role>
Name:<role>
```

Rules:

- `main` resolves to the main thread for the current family.
- `subagent` resolves only when exactly one open subagent exists.
- `subagent:N` is family-local and stable by host graph order.
- Bare names and roles resolve only when unambiguous.
- The main thread is addressed as `main`, not by title or nickname.
- Hosted tools must not accept arbitrary raw thread ids from the model.
- Default `team` must not infer delivery capability unless the host reports it.

Latency budget:

```text
P50 <= 100ms
P95 <= 500ms
default-output timeout <= 500ms
```

Timeout or unavailable host runtime must return an explicit unavailable result.
Do not fall back to SQL snapshots.

## Write

Schema:

```ts
agent_mail.write(args: {
  to: string
  body: string
  deliveryMode?: "queue" | "interrupt"
  forwardNext?: number
  requireReply?: boolean
}): AgentMailReceipt
```

Defaults:

```text
deliveryMode = "queue"
forwardNext = 3
requireReply = false
```

Strict input rules:

- Reject sender fields such as `from`, `fromThreadId`, `sender`, and
  `senderThreadId`.
- Reject unknown fields.
- Reject empty body unless a future typed non-body event explicitly permits it.

Delivery flow:

```text
1. Derive sender identity from the active host session.
2. Resolve `to` through the live team resolver.
3. Create an Agent Mail message id and receipt record.
4. Build InterAgentCommunication with host-derived author and recipient paths.
5. Enqueue through the recipient InputQueue.
6. Emit or persist a native Agent Mail GUI item.
7. Return a receipt only after host-native enqueue/render state is known.
```

Queue mode:

```text
InterAgentCommunication.trigger_turn = false
```

Queue mode must not interrupt, start, steer, or fake a turn. It should create
visible GUI mail and wait for the recipient's normal mailbox-drain boundary.

Interrupt mode:

```text
deliveryMode = "interrupt"
```

Interrupt mode is explicit. It may use `trigger_turn = true`, a host interrupt
operation, or both according to Codex turn policy. It must still create a real
mail item and receipt.

## Forwarded Context

`forwardNext` captures the sender's next visible GUI/thread messages and sends
them as one labeled follow-up context item.

Rules:

- Default `forwardNext` is 3.
- `forwardNext: 0` disables forwarding.
- Cap to 5.
- Forward only visible messages, never hidden reasoning.
- Stop after count, recipient reply, or short expiry.
- Render as forwarded context, not fake user/model turns.
- Follow-up context interrupts by default because it often corrects or refines
  the initial note.

## Require Reply

`requireReply` marks accountability. It must not block the sender's turn by
default and must not reintroduce a public Agent Mail `wait` command.

Receipt state may include:

```text
replyRequired
replyReceived
replyThreadId
replyAt
```

## Read

Schema:

```ts
agent_mail.read(args: {
  target: string
  limit?: number
  since?: string
  until?: string
}): AgentTranscriptView
```

Defaults:

```text
limit = 10
max limit = 100
```

UTC `since` and `until` windows are inclusive.

`read` is live transcript browsing for a reachable target. It is not an inbox
check and must not pretend mailbox state is transcript state.

Forbidden read sources:

```text
SQLite snapshots
stale thread rows
terminal UI scraping
global history scans
local mailbox files
```

## GUI Items

Agent Mail needs a native GUI item or equivalent app-server protocol item with:

```text
id
from identity
to identity
body
deliveryMode
requireReply
receipt status
createdAt
deliveredAt
readAt
forwardedContext metadata
```

Existing Codex `CollabAgentToolCall` proves the app-server can render native
inter-agent activity, but it is not a complete mail object. Do not overload
tool-call prompt text as durable mail state.

Queue vs interrupt is delivery behavior, not object type.

## Receipt Model

At minimum:

```text
resolved
enqueued
rendered
failed
```

Future optional states:

```text
deliveredToTurn
read
replyReceived
```

Do not return success from hosted `write` unless the host can prove native
enqueue and GUI item/receipt state.

## Capability Probe

Schema:

```json
{
  "version": 1,
  "nativeGuiMail": true,
  "identityRendering": true,
  "readUnreadState": true,
  "interruptMode": true,
  "forwardNext": true
}
```

External bridge clients such as `agent-mail` must probe before calling native
write/read APIs. If missing or `nativeGuiMail: false`, CLI `write/read` must
fail clearly and keep directory/debug commands available.

## CLI And Plugin

CLI commands:

```text
agent-mail team
agent-mail write <target> <message>
agent-mail read <target>
agent-mail hook <event>
```

Compatibility aliases for `team`:

```text
subagents
coworkers
contacts
ls
```

Until hosted APIs exist:

- `team` may remain a debug bridge.
- `write` and `read` must fail clearly after target resolution.
- CLI must not create local mailboxes, scrape transcripts, inject items, or
  claim GUI-native delivery.

Hook copy must be short and must not imply CLI delivery:

```text
Agent Mail is native-GUI-first. Use `agent-mail team` to see this main/subagent family; use hosted `agent_mail.write/read` for real mail when Codex exposes them.
```

## Non-Goals

```text
local mailbox fallback
SQLite snapshots as product reachability
thread/inject_items as delivery proof
turn/start or turn/steer as queue mail
terminal transcript scraping
hidden detached sessions
shelling out from Codex core to agent-mail
```

## Acceptance Tests

- `agent_mail.team` returns only the current live family.
- `agent_mail.team` times out under 500ms and does not fall back to SQL.
- `main` resolves from both main and subagent contexts.
- `subagent` resolves only when exactly one subagent exists.
- `subagent:N` remains stable as transcript activity changes.
- Unique names resolve; duplicate names require suffixes.
- Role aliases resolve only when unique.
- Model-supplied sender fields are rejected.
- Queue write enqueues native inter-agent communication with no interrupt.
- Queue write emits or persists a GUI mail item.
- Interrupt write uses explicit interrupt/wake policy and still records mail.
- Default write captures up to three forwarded visible messages.
- `forwardNext: 0` disables forwarded context.
- `requireReply` marks state without blocking the sender turn.
- `read` returns latest 10 visible transcript messages by default.
- UTC `since` and `until` windows are honored inclusively.
- CLI `write/read` fail clearly without native host support.
- No implementation path uses local mailbox files, `thread/inject_items`,
  `turn/start`, or `turn/steer` as queue mail.
