# agent-mail

`agent-mail` is a small command-line interface for mailing Codex coagents.

It is not a workflow engine. It has one job: make it easy to find agents in the
current family, send them live messages, and read enough conversation history to
know whether the message landed.

## Command Set

```bash
agent-mail ls
agent-mail to <target> "<message>"
agent-mail read <target>
agent-mail watch <target>
agent-mail search "<query>"
```

That is the core surface.

## Targets

`agent-mail ls` lists only the current family by default:

```text
ROLE      TITLE / NAME              ID                                      STATUS
self      Current side agent         019e...                                 active
parent    Main task agent            019e...                                 active
child     Docs worker                019e...                                 running
child     Bug hunter                 019e...                                 completed
```

The target resolver accepts identifiers from `ls`:

```text
self
parent
child
child:1
child:docs
Docs worker
019e96ef-9c20-...
```

If `ls` can see it, `to`, `read`, and `watch` should accept it.

Cross-session agents are intentionally outside the default family view. Use
`agent-mail search "<query>"` to find them in broader session logs, then address
them by explicit thread or agent id.

## Sending Mail

```bash
agent-mail to parent "focus on the live A/V presenter bug"
agent-mail to child:1 "rerun the smoke test and tell me the first failure"
```

Default behavior:

1. Resolve the target.
2. Send a live message through the best available agent channel.
3. Print the delivery id or clear failure.
4. Read back enough target history to show the message landed or is pending.

File handoffs are not part of the core command set. If a live send path is not
available, the tool should say that directly instead of pretending a file note is
mail.

## GUI-Visible Mail

Some Codex steering messages appear in the GUI as a collapsed "Steered
conversation" item. When the sender needs the body visible in the target chat,
use `--visible`:

```bash
agent-mail to child:1 --visible "hello from side agent"
```

The message wrapper should ask the recipient to echo the body near the top of
its next visible assistant update:

```text
Mail from side agent. Please echo this message body near the top of your next
visible assistant update, then continue your current task:

hello from side agent
```

This is intentionally explicit. GUI visibility is different from delivery.

## Reading Mail

```bash
agent-mail read parent
agent-mail read child:1 --limit 20
```

Default output is role-only visible transcript:

```text
user:
...

assistant:
...
```

Tool calls, edit summaries, and raw rollout events are hidden unless requested.

Optional flags:

```bash
agent-mail read child:1 --events
agent-mail read child:1 --timestamps
```

## Watching Mail

```bash
agent-mail watch child:1
agent-mail watch child:1 --timeout 10m
```

`watch` follows target conversation activity until one of these happens:

- new assistant output appears
- the target reaches a completed state
- timeout expires

It should not busy-poll aggressively.

## Search

```bash
agent-mail search "Document RTL workflow"
agent-mail search "A/V presenter C2H"
```

Search is wider than `ls`. It can inspect session indexes, process-manager
state, rollout names, and recent transcript text. Search results should mark
whether a target is in-family or cross-session.

```text
MATCH     SCOPE          TITLE                  ID
1         in-family      Docs worker             019e...
2         cross-session  Old bug hunt            019e...
```

## Closed-Loop Behavior

The tool should be bold by default but honest about proof.

For `to`, success means the message was submitted to a live channel. Stronger
success means `read` or `watch` shows either:

- the message body appeared visibly, or
- the target's next assistant response changed behavior in response to it.

For `--visible`, success should include the echoed body in the transcript.

The tool should print the distinction:

```text
submitted: 019e...
visible: yes
target_changed_behavior: unknown
```

## Non-Goals

- No workflow DSL.
- No mandatory aliases.
- No durable file-handoff fallback pretending to be mail.
- No default scan of every historical Codex session.
- No hidden mutation of unrelated threads.

## Initial Implementation Notes

Likely data sources:

- Codex process-manager state for active family members.
- Codex session index for titles and thread ids.
- Rollout JSONL files for visible `user` / `assistant` transcript history.
- Built-in multi-agent send APIs for live child-agent delivery.
- Codex app-server APIs for explicit cross-session sending when safe.

The implementation should start with `ls`, `to`, and `read`. Add `watch` and
`search` once the basic live path is reliable.
