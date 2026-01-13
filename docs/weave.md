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
  - “Switch profile” (a second identity)
  - “Set agent color”

Sessions are treated as “rooms”. For “breakouts”, create/join additional sessions (e.g. `main/design`).

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

