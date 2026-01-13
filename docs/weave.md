# Weave collaboration

Weave lets multiple Code instances (separate terminals) coordinate by joining a shared session and sending agent-to-agent messages over a local Weave coordinator.

## Requirements

- **macOS/Linux only** (uses a Unix domain socket).
- A **Weave coordinator** running locally.
  - Default socket: `~/.weave/coord.sock`
  - Override: set `WEAVE_HOME=/path/to/dir` (both the coordinator and Code must use the same value).

## Quickstart (local test)

1. Start the Weave coordinator (separate terminal):

   - macOS: `weave-service start`
   - Stop later with: `weave-service stop`

   If `weave-service` is not installed, you can install it via:
   - `npm install -g @rosem_soo/weave`

   Verify the socket exists:
   - `ls ~/.weave/coord.sock`

2. Start **two** Code instances (two terminals):

   In each terminal:
   - `cd /path/to/your/code/repo`
   - `./build-fast.sh run`

3. In each Code instance, connect to the same Weave session:

   - Type `/weave` to open the Weave menu.
   - Pick **Set agent name** (use a single token like `alice` / `bob`).
   - In the first terminal, pick **Create new session**.
   - In the second terminal, pick the session from the list to join it.

4. Send a Weave message using `#mentions`:

   From `alice`, type a normal message containing a mention token:

   - `#bob Please take a look at this.`

   Notes:
   - Mentions must be **standalone, whitespace-separated tokens** (use `#bob`, not `#bob,`).
   - Mention matching is **case-sensitive**.
   - Names with spaces are not mentionable; use single-token names.

5. Confirm it worked:

   - The sender sees an outbound message with the `"weave"` header and a `⇄` gutter marker.
   - The recipient sees an inbound message with the same `"weave"` header.
   - Agent names are colored consistently (based on agent id; restarting Code changes colors).

## `/weave` command reference

- `/weave` (or `/weave menu`): open Weave session menu.
- `/weave name <name>`: set your agent name.
- `/weave create <session name>`: create + join a new session.
- `/weave join <session id>`: join an existing session.
- `/weave leave`: disconnect from the current session.
- `/weave close <session id>`: close a session.
- `/weave refresh`: refresh the agent list for mention routing.
- `/weave help`: show quick help.

## Troubleshooting

- **“Weave coordinator socket not found …/coord.sock”**
  - Start the coordinator and re-run `/weave`.
  - Confirm `WEAVE_HOME` (if set) matches between coordinator and Code.
- **My `#mention` went to the model instead of Weave**
  - You are not connected to a session, or the mention didn’t match a known agent.
  - Use `/weave` to confirm you’re connected, then try `/weave refresh`, and re-send with an exact `#name` token.
