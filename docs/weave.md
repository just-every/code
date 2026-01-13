# Weave collaboration

Weave lets multiple Code instances (separate terminals) coordinate by joining a shared session and sending agent-to-agent messages over a local Weave coordinator.

## Requirements

- **macOS/Linux only** (uses a Unix domain socket).
- A **Weave coordinator** running locally.
  - Default socket: `~/.weave/coord.sock`
  - Override: set `WEAVE_HOME=/path/to/dir` (both the coordinator and Code must use the same value).

Notes

- When copying commands from docs/chat, do **not** include the surrounding Markdown backticks (`). In your shell, backticks mean “command substitution” and will break the commands.
- Avoid `sudo` for Weave; it can create sockets owned by `root` and prevent Code from connecting.

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

   Optional (recommended): set a stable Weave profile name per terminal before starting Code:
   - `export CODE_WEAVE_PROFILE=alice` (in one terminal)
   - `export CODE_WEAVE_PROFILE=bob` (in the other terminal)
   - Use a different value per Code instance; sharing a profile makes both terminals act as the same agent.

3. In each Code instance, connect to the same Weave session:

   - Type `/weave` to open the Weave menu.
   - Pick **Set agent name** (use a single token like `alice` / `bob`).
     - If the name is already taken in the session, Code auto-suffixes it (e.g. `alice-2`) and shows a notice.
   - In the first terminal, pick **Create new session**.
   - In the second terminal, pick the session from the list to join it.

4. Send a Weave message using `#mentions`:

   From `alice`, type a normal message containing a mention token:

   - `#bob Please take a look at this.`

   Notes:
   - Mentions must be **standalone, whitespace-separated tokens** (e.g. `#bob`).
   - Mention matching is **case-insensitive** and ignores trailing punctuation (e.g. `#BoB,` works).
   - Names with spaces are not mentionable; use single-token names.
   - While typing `#...`, Code shows an autocomplete popup. Use `↑/↓` to select and `Tab`/`Enter` to insert.

5. Confirm it worked:

   - The sender sees an outbound message with the `"weave"` header and a `⇄` gutter marker.
   - The recipient sees an inbound message with the same `"weave"` header.
   - The bottom-right footer shows `Weave: <agent> • <session> (connected)`.
   - Agent names are colored consistently (based on agent id). You can also override via **Set agent color** in the `/weave` menu.

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
  - Use `/weave` to confirm you’re connected, then try `/weave refresh` so autocomplete has the latest agent list.
  - Mentions must be whitespace-separated tokens (e.g. `#bob` / `#bob,`), not embedded in other text.
