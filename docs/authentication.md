# Authentication

## Usage-based billing alternative: Use an OpenAI API key

If you prefer to pay-as-you-go, you can still authenticate with your OpenAI API key by setting it as an environment variable:

```shell
export OPENAI_API_KEY="your-api-key-here"
```

Alternatively, read from a file:

```shell
code login --with-api-key < my_key.txt
```

The legacy `--api-key` flag now exits with an error instructing you to use `--with-api-key` so that the key never appears in shell history or process listings.

This key must, at minimum, have write access to the Responses API.

## Migrating to ChatGPT login from API key

If you've used the Code CLI before with usage-based billing via an API key and want to switch to using your ChatGPT plan, follow these steps:

1. Update the CLI and ensure `code --version` is `0.5.0` or later
2. Delete `~/.code/auth.json` (and remove the legacy `~/.codex/auth.json` if it exists; on Windows these live under `C:\\Users\\USERNAME\\.code\\auth.json` and `C:\\Users\\USERNAME\\.codex\\auth.json`)
3. Run `code login` again

## Forcing a specific auth method (advanced)

You can explicitly choose which authentication Code should prefer when both are available.

- To always use your API key (even when ChatGPT auth exists), set:

```toml
# ~/.code/config.toml (Code also reads legacy ~/.codex/config.toml)
preferred_auth_method = "apikey"
```

Or override ad-hoc via CLI:

```bash
code --config preferred_auth_method="apikey"
```

- To prefer ChatGPT auth (default), set:

```toml
# ~/.code/config.toml (Code also reads legacy ~/.codex/config.toml)
preferred_auth_method = "chatgpt"
```

Notes:

- When `preferred_auth_method = "apikey"` and an API key is available, the login screen is skipped.
- When `preferred_auth_method = "chatgpt"` (default), Code prefers ChatGPT auth if present; if only an API key is present, it will use the API key. Certain account types may also require API-key mode.
- To check which auth method is being used during a session, use the `/status` command in the TUI.

## Project .env safety (OPENAI_API_KEY)

By default, Code will no longer read `OPENAI_API_KEY` or `AZURE_OPENAI_API_KEY` from a project’s local `.env` file.

Why: many repos include an API key in `.env` for unrelated tooling, which could cause Code to silently use the API key instead of your ChatGPT plan in that folder.

What still works:

- `~/.code/.env` (or `~/.codex/.env`) is loaded first and may contain your `OPENAI_API_KEY` for global use.
- A shell-exported `OPENAI_API_KEY` is honored.

Project `.env` provider keys are always ignored — there is no opt‑in.

UI clarity:

- When Code is using an API key, the chat footer shows a bold “Auth: API key” badge so it’s obvious which mode you’re in.

## Connecting on a "Headless" Machine

Today, the login process entails running a server on `localhost:1455`. If you are on a "headless" server, such as a Docker container or are `ssh`'d into a remote machine, loading `localhost:1455` in the browser on your local machine will not automatically connect to the webserver running on the _headless_ machine, so you must use one of the following workarounds:

### Authenticate locally and copy your credentials to the "headless" machine

The easiest solution is likely to run through the `code login` process on your local machine such that `localhost:1455` _is_ accessible in your web browser. When you complete the authentication process, an `auth.json` file should be available at `$CODE_HOME/auth.json` (defaults to `~/.code/auth.json`; Code will still read `$CODEX_HOME`/`~/.codex/auth.json` if present).

Because the `auth.json` file is not tied to a specific host, once you complete the authentication flow locally, you can copy the `$CODE_HOME/auth.json` file to the headless machine and then `code` should "just work" on that machine. Note to copy a file to a Docker container, you can do:

```shell
# substitute MY_CONTAINER with the name or id of your Docker container:
CONTAINER_HOME=$(docker exec MY_CONTAINER printenv HOME)
docker exec MY_CONTAINER mkdir -p "$CONTAINER_HOME/.code"
docker cp auth.json MY_CONTAINER:"$CONTAINER_HOME/.code/auth.json"
```

whereas if you are `ssh`'d into a remote machine, you likely want to use [`scp`](https://en.wikipedia.org/wiki/Secure_copy_protocol):

```shell
ssh user@remote 'mkdir -p ~/.code'
scp ~/.code/auth.json user@remote:~/.code/auth.json
```

or try this one-liner:

```shell
ssh user@remote 'mkdir -p ~/.code && cat > ~/.code/auth.json' < ~/.code/auth.json
```

### Connecting through VPS or remote

If you run Code on a remote machine (VPS/server) without a local browser, the login helper starts a server on `localhost:1455` on the remote host. To complete login in your local browser, forward that port to your machine before starting the login flow:

```bash
# From your local machine
ssh -L 1455:localhost:1455 <user>@<remote-host>
```

Then, in that SSH session, run `code` and select "Sign in with ChatGPT". When prompted, open the printed URL (it will be `http://localhost:1455/...`) in your local browser. The traffic will be tunneled to the remote server.
