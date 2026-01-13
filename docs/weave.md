# Weave (local multi-terminal messaging)

This branch adds a simple Weave client to the TUI so multiple `code` instances can DM each other (and share the same session as a “room”) on your local machine.

## Prereqs

- `weave-service` installed and running.
- `WEAVE_HOME` (optional). Defaults to `~/.weave`.

Quick sanity check:

```bash
ls -l ~/.weave/coord.sock
```

## Run (two terminals)

Terminal A:

```bash
cd ~/dev/code
./build-fast.sh run
```

Terminal B (same):

```bash
cd ~/dev/code
./build-fast.sh run
```

## Connect

In each terminal:

- Type `/weave` to open the Weave menu.
- Pick an existing session (or “Create new session”).
- Optionally set:
  - “Set agent name”
  - “Switch profile” (multiple personas/identities)
  - “Auto mode” (off/reply/work)
  - “Persona memory” (persistent notes for this profile)
  - “Set agent color”

## Rooms + breakout rooms

Weave sessions are treated as “rooms”. For “breakouts”, create/join additional sessions with hierarchical names (e.g. `main/design`, `main/bugfix/ui`).

In the `/weave` menu:

- Breakouts are shown indented under their parent.
- “Create breakout room” prefills `/weave create <room>/` in the composer.

## DMs (`#mention`)

Once both terminals are in the same session:

- In Alice’s terminal, type `#bob Good morning`.
- Bob’s terminal should show an inbound chat message from Alice.
- Type `#alice ...` in Bob to reply.

Autocomplete: type `#` to see mention candidates; use the normal selection/accept keys to insert.

If `#bob ...` is treated like a normal prompt, Weave likely hasn’t loaded the agent list yet; wait a moment or run `/weave refresh`.

## Delivery receipts

For **single-recipient** DMs, outbound messages show a small status line:

- `Sending…` → `Sent` → `Delivered` → `Seen`

Receipts are local-only control messages and are not shown in the transcript.

## Auto mode (auto-reply / autorun)

Enable auto mode in Bob’s terminal:

- `/weave auto reply` — auto-respond with chat only (no tools/commands).
- `/weave auto work` — auto-respond and work on tasks (may request approvals).
- `/weave auto off` — disable.

When auto mode is enabled, incoming Weave DMs are queued and processed when the terminal is idle.

## Persona memory

Persona memory is persisted per Weave profile and injected into auto mode prompts.

- `/weave memory` — edit memory (multi-line prompt)
- `/weave memory clear` — clear memory
