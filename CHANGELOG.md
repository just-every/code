# Changelog

> [!TIP]
> We're constantly improving Code! This page documents the core changes. You can also check our [releases page](https://github.com/just-every/code/releases) for additional information.

## [Unreleased]

- (none)

## [0.6.47] - 2026-01-13

- Core/Websocket: reuse connections and add append support to cut reconnect churn. (d75626a, e726a82)
- MCP: hot reload servers with static callbacks for smoother development. (3e91a95c, d5562983)
- CLI: add --url with OAuth-friendly defaults and prompt Windows users on unsafe commands. (145974c4, ddae70bd)
- TUI: keep scrollback tails visible and show in-flight coalesced tool calls. (7c1e7747, 602ddfee, 273ba32d, 12779c7c)
- Exec/Auto: honor reasoning effort and dedupe reasoning output during auto runs. (bc81e7c2, 00fbc71a)

## [0.6.46] - 2026-01-11

- TUI/Stream: preserve commit ticks while debouncing to keep command ordering intact. (365bf7a)
- TUI/Render: resync buffers after WouldBlock errors so redraws recover cleanly. (7ef6a6c)

## [0.6.45] - 2026-01-09

- TUI/Render: clear after WouldBlock redraws to resync the terminal and remove stale tail lines. (a354fdf8)
- TUI/Render: improve redraw stability under terminal backpressure so frames recover cleanly. (a354fdf8)

## [0.6.44] - 2026-01-08

- TUI/Render: reset skip flags when filling backgrounds so reused buffer cells redraw correctly. (035abd0d)
- TUI/Render: ensure background fill without characters also clears skip to prevent lingering artifacts. (035abd0d)

## [0.6.43] - 2026-01-08

- TUI/Images: guard dropped images and clipped views so broken files fall back to placeholders. (b76b2455)
- TUI/Images: avoid partial rendering on graphic protocols to prevent cursor corruption while scrolling. (b76b2455)

## [0.6.42] - 2026-01-08

- TUI/Images: persist pasted or dropped images from temp locations into session storage so they stay available when sending. (00cbdfc)
- TUI/Composer: keep image placeholders verbatim when building messages so inline markers align with attachments. (00cbdfc)

## [0.6.41] - 2026-01-08

- TUI/History: show exec and MCP cards immediately and drop spacer after collapsed reasoning before exec. (3c68afba, fcb48a71)
- Exec: send prompt and images in one turn to keep runs aligned. (f23e562f)
- TUI/Queue: dispatch queued input immediately so interactions start without delay. (b00b6ce3)
- TUI/Render: preserve WouldBlock kind in draw errors for accurate diagnostics. (5faf4411)

## [0.6.40] - 2026-01-07

- TUI/Image: initialize picker state for image cards so selection works reliably. (63e53af9)
- Core: gate cgroup helpers on Linux to avoid non-Linux builds invoking them. (7369620a)

## [0.6.39] - 2026-01-07

- TUI/Auto-drive: add navigation telemetry and forward aligned compacted history for new browser runs. (94eb8d23, d6d52b7f, aaac24f7)
- TUI2/Markdown: stream logical lines so transcripts reflow correctly on resize and copy/paste. (c92dbea7)
- TUI: render view-image paths relative to the working directory for non-git projects. (4c3d2a5b)
- TUI2/Transcript: add an auto-hiding scrollbar, anchor the copy pill at the viewport bottom, and cache rendering to cut redraws. (8f10d3bf, 56782130, 90f37e85)

## [0.6.38] - 2026-01-06

- Docs/Config: clarify `--model` applies to the active provider and call out OpenAI-compatible requirements for custom providers. (fa6c482)
- Docs/Config: add a proxy example for routing OpenAI-style requests to other vendors. (fa6c482)

## [0.6.37] - 2026-01-06

- TUI/Image: render view image cards so attached visuals show inline. (658ddfb)
- TUI/Browser: scope console logs to each browser card to avoid spillover. (e1d8f12)
- TUI/Resume: prevent footer underflow in resume layouts. (deef15a)
- TUI/Composer: guard composer height to keep the input stable. (041dff4)
- Core/Config: allow tool output size override to honor config limits. (c3782ba)

## [0.6.36] - 2026-01-05

- TUI: prioritize task cancellation on Esc before agent input to make stopping runs reliable. (76e3dd8)
- Tests: reduce linux sandbox and TUI timeout flakes for steadier CI runs. (9d5dbfc)

## [0.6.35] - 2026-01-05

- Core/Agent: keep packaged code executable available for read-only agents to avoid missing-binary failures. (d1557c5)
- Core/Agent: fall back to local dev build when the running binary disappears to keep agent commands working. (d1557c5)

## [0.6.34] - 2026-01-05

- Core/Auth: auto-switch to another account when usage limits hit to keep runs moving. (590f46be)
- UX: show notices when accounts auto-switch due to rate limits so users stay informed. (590f46be)

## [0.6.33] - 2026-01-05

- TUI: keep cancelable agents prioritized when Esc is pressed. (195c768)
- TUI: prompt to init git before git-dependent actions run. (6d693c3f)
- Logging: overhaul debug log handling for clearer diagnostics. (afba3b9c)

## [0.6.32] - 2026-01-04

- TUI: prevent Esc undo priming from sticking and stabilize word-motion shortcuts. (d90b0f9)
- TUI: refactor Esc handling into a dedicated module for clearer behavior. (d90b0f9)

## [0.6.31] - 2026-01-04

- Core/Config: add missing test imports to keep config checks stable. (97672cc7)
- TUI/Logging: throttle frame timer spam to reduce noisy redraw logs. (3eeef61c)
- Core/TUI: split large modules to improve stability and maintainability. (5c9d9743)

## [0.6.30] - 2026-01-04

- TUI/Auto Drive: avoid full render rebuilds to cut redraw overhead during runs. (6db1e0b)
- TUI/History: cache patch summary layout to reduce churn and flicker. (845d63e)
- TUI/Logs: throttle thread spawn errors to prevent repeated warnings. (e91ce8c)

## [0.6.29] - 2026-01-02

- TUI/Markdown: wrap wide code graphemes to avoid overflow in rendered blocks. (8519507)
- TUI/Markdown: flush wrapped code rows so virtualized views stay aligned. (b4d8264)

## [0.6.27] - 2026-01-02

- Auto-review: skip re-reviews when files are unchanged to cut noise. (0dddef81)
- Core/GH: auto-resolve gh_run_wait defaults for smoother release checks. (a865bee5)

## [0.6.26] - 2026-01-01

- Image view: render local image attachments in transcripts and tools. (71a68a2)
- Core/Tools: align image_view tool coverage to match supported sources. (fcb3afb)

## [0.6.25] - 2026-01-01

- CLI/GH: use GitHub run URLs when waiting on Actions to avoid stale links. (a7f6402a)
- CLI/GH: show wait details while following GitHub Actions runs for clarity. (a7f6402a)

## [0.6.24] - 2026-01-01

- TUI: keep virtualization frozen for tail-only views to avoid redraw churn. (77f37f33)
- TUI: defer virtualization sync until the view is ready to prevent flicker. (6c11ec70)
- Core/GH: allow gh_run_wait to target specific repos for release monitoring. (a83514b4)

## [0.6.23] - 2025-12-31

- TUI: align welcome layout height with width to keep the intro balanced. (36aef09f)
- TUI: stabilize welcome intro sizing across resolutions to avoid jitter. (6b4cebed, 117863e2)

## [0.6.22] - 2025-12-31

- Agents: wake on batch completion to avoid stalled automation runs. (0c461689)
- Core: refresh codex-rs mirror to upstream main to stay aligned with engine updates. (92641d9f)
- Deps: bump tokio, tracing-subscriber, toml_edit, regex-lite in codex-rs for stability. (a48904de, 4313e0a7, ce3ff299, 13c42a07)

## [0.6.20] - 2025-12-30

- Auto Drive: keep retrying after errors so runs recover instead of stopping early. (7f6c12e8)
- Auto Drive: schedule restarts without depending on Tokio to avoid stalled recoveries. (bae785e9)

## [0.6.19] - 2025-12-29

- Agents: default built-in slugs to code-gpt-5.2-codex for faster, higher-quality automation. (8afe9b8c)
- Agents: expand GPT-5 alias coverage and docs so configs map cleanly to the new defaults. (8afe9b8c)

## [0.6.18] - 2025-12-28

- TUI: add `/skills` slash command to list available skills inline. (7087feb)
- Exec: handle missing wait output to keep execution results consistent. (d1cc1a2)
- Auto Drive: stop runs after fatal errors to avoid hanging sessions. (a481b54)

## [0.6.17] - 2025-12-28

- TUI2: improve transcript selection with multi-click, drag start, copy shortcut, and corruption fixes when copying offscreen text. (0130a2fa, 28285493, 414fbe0d, 310f2114, 7d0c5c7b)
- Auto Drive: keep agent runs alive and clamp overlays to avoid misaligned prompts. (eafae4bc, 7b28c36b)
- Config: honor /etc/codex/config.toml, in-repo config sources, and project_root_markers for workspace detection. (e27d9bd8, 8ff16a77, 314937fb)
- Exec/CLI: limit unified exec output size and improve ripgrep download diagnostics for clearer failures. (fb24c47b, f2b740c9)
- Performance: cache history render requests and cap redraw scheduling to 60fps to reduce TUI CPU usage. (72b6650f, 96a65ff0)

## [0.6.16] - 2025-12-25

- Auto Drive: tighten timeboxed coordinator guidance so runs lead with authoritative verifiers and outcome-only directives. (d3efecb)
- CLI: expand timeboxed exec guidance to force early acceptance checks and proof before finishing. (d3efecb)

## [0.6.15] - 2025-12-24

- Exec: add timeboxed auto-exec guidance to keep runs bounded. (8dbfdbba)
- Auto Drive: tighten time budget guidance and drop unused seed to reduce noise. (376fc8ff, 736e6cf0)

## [0.6.14] - 2025-12-23

- TUI: clear stale mid-turn output when starting a new task so history stays accurate. (dd610fe2)
- TUI: clear exec spinners when a TaskComplete event is missing to avoid stuck indicators. (e047feb4)
- Core/Auth: switch the active account based on session context to honor workspace permissions. (ac958448)
- Browser: restart the navigation handler after repeated errors to restore browsing. (940dcc44)
- Auto-review: defer baseline capture to keep automated review diffs stable. (6818c0b5)

## [0.6.13] - 2025-12-22

- TUI: add account switching and skills settings in the core UI. (bcf7614a)
- TUI2: normalize wheel and trackpad scrolling for consistent transcript navigation. (63942b88)
- Auto-drive: improve ansi16 styling, contrast, and ghost snapshot timing for clearer prompts. (ffac25b4, 82ce5915, 6e333af8, 70e1b9f6)
- Auto-drive: respect retry budgets and loop hints to avoid runaway retries. (f5c56198)
- Stability: prevent panics on alpha builds. (aa83d7da)

## [0.6.12] - 2025-12-20

- TUI: coalesce transcript redraws, keep spinners live, and shorten status directory labels so streams stay smooth. (1d4463ba, 734dd0ee, e6794b7b)
- Exec: reduce long-session stalls and collapse waiting time in unified exec so commands finish faster. (5cfb8309, 6c76d177)
- CLI: add `/ps` and apply terminal-aware scroll scaling for clearer process visibility. (4fb0b547, df46ea48)
- Config/Skills: backport requirements updates, add ExternalSandbox policy, and support `/etc/codex/requirements.toml` for tighter governance. (f2750fd6, 3429de21, 2f048f20)

## [0.6.10] - 2025-12-18

- TUI: keep bulk command processing responsive during heavy redraw bursts. (9cf56083)
- Performance: prevent redraw loops from starving queued work so outputs stay timely. (9cf56083)

## [0.6.9] - 2025-12-18

- TUI/cards: set ANSI-16 card backgrounds for consistent styling. (820991e9)
- TUI/status: restore the Every Code header title in the status view. (455ed636)

## [0.6.8] - 2025-12-18

- Agents: default frontline automation to gemini-3-flash for faster runs. (bfc41e28)
- TUI/Auto Drive: normalize footer hint spacing so prompts align cleanly. (8da0bda7)

## [0.6.7] - 2025-12-17

- Core/TUI: add remote model support and harden exec memory handling for safer runs. (db385786)
- Auto Drive: summarize the last session on completion so users get a quick recap. (86f691d)
- Exec: add a max-seconds budget with countdown nudges and clean up log paths for killed children. (0c323447, f1835d5)
- Reliability: auto-retry turns after usage limits and avoid cloning large histories during retention cleanup. (220414d, b91e303)

## [0.6.6] - 2025-12-15

- TUI: show Every Code title and stabilize header rendering so status bar and snapshots stay consistent. (1f77f7ac, a8b8beeb)
- Skills: reimplement loading via SkillsManager and add skills/list op for more reliable discovery. (5d77d4db)
- Config: clean config loading/API, expand safe commands, and refresh disk status using latest values for MCP servers. (92098d36, 49bf49c2, 163a7e31)
- Windows: locate pwsh.exe/powershell.exe reliably and parse PowerShell output with PowerShell for sturdier scripts. (4312cae0, 90094903)
- MCP/TUI: restore startup progress messages and show xhigh reasoning warnings for gpt-5.2 to keep users informed. (c978b6e2, 9287be76)

## [0.6.4] - 2025-12-13

- TUI: default hidden-preface injections to silent and allow silent submissions to reduce demo noise. (b1ae6f4, a086a46)
- CLI demo: add a --demo developer message injection flag for scripted demos. (6b2e61b)
- TUI: dim mid-turn assistant output and improve plan/cursor contrast in dark mode for clearer streams. (b688046, 1b8c8fd)
- Exec: add a grace delay before shutdown when auto-review is enabled to avoid abrupt stops. (c6d6f49)
- TUI: hide the directory label in demo mode for cleaner status displays. (c03b3e9)

## [0.6.3] - 2025-12-12

- Build: prevent concurrent tmp-bin races in build-fast to keep artifacts isolated. (e3ae904)
- TUI history: handle background wait call_id to avoid orphaned exec entries. (c4c4ed6)
- Onboarding: align trust directory prompt styling with the rest of the flow. (9de6df6)

## [0.6.2] - 2025-12-12

- Models: add a guided gpt-5.2 upgrade flow so users can move to the latest model smoothly. (ee9fc1f)
- TUI history: keep mid-turn answers ordered, hide stray gutters, and collapse duplicate reasoning streams for cleaner transcripts. (89e485d, 0991bf2, fe40d37, d79442a)
- Exec: guard process spawns, pair early exec ends with begins, and keep live output flowing while capping previews to avoid hangs. (1e66674, 66e650c, d1a36f0, e780d82)
- TUI: allow user input to interrupt wait-only execs and force redraws after backpressure stalls for more responsive UI. (bcbcb95e, 5780d0d)
- Snapshots: warn when snapshots run long and add a shell command snapshot path. (b2280d6, 29381ba)

## [0.6.1] - 2025-12-10

- Auto Review: Harden locks, fallback worktrees, zero-count status, and pending-fix isolation so automated reviews stay reliable. (44234af, a62eaa3, 609432e, ae4b4ec)
- Models: Introduce ModelsManager across app server and TUI, add a remote models flag, and cache disk-loaded presets with TTL/ETag for faster selection. (00cc00e, 8da91d1, 53a486f, 222a491)
- Shell & Exec: Detect mutating commands, snapshot shell state, and clear lingering execs so automation captures side effects cleanly. (da983c1, 7836aed, cf15065)
- TUI UX: Add vim-style pager keys, Ctrl+N/P list shortcuts, tighter shell output limits, and aligned auto-review footers for smoother navigation. (9df70a0, 4a3e9ed, 3395ebd, 9e2b68b)

## [0.5.15] - 2025-11-28

- CLI: bump npm metadata to 0.5.15 so fresh installs pull the latest binaries. (0e5d3f9)
- CI: enforce running ./pre-release.sh before pushes to main to keep release checks green. (3af8354)

## [0.5.14] - 2025-11-28

- Core/Bridge: surface code-bridge events directly in sessions so runs show live bridge activity. (ca8f0efa)
- TUI: keep composer popups aligned after history navigation and wrap the agent list inside the command editor for better readability. (bb4a43cf, b890eac3)
- Auto Drive: stabilize the intro placeholder and ensure exec completions render in order so automation transcripts stay coherent. (7a652b74, d9e5ddbd)
- Core/Compact: prune orphan tool outputs before compaction to shrink bloated histories and speed up resumes. (8ba5f744)

## [0.5.13] - 2025-11-27

- CLI: bump the npm package and platform binaries to 0.5.13 so installs grab the latest build. (285c8ca7)
- CI: add a placeholder rust-ci workflow so required checks stay green during migration. (6d6ee6cf)

## [0.5.12] - 2025-11-27

- CLI: bump npm metadata to 0.5.12 so fresh installs pull the latest binaries. (9f79140)
- CI: drop the redundant CodeQL workflow to stop conflicting security scans. (ff71e00)

## [0.5.10] - 2025-11-27

- CLI: bump the npm package metadata to 0.5.10 so installs pick up the latest build. (16b47ef)
- CI: add a Ruby-free CodeQL workflow to keep security scanning enabled without extra dependencies. (ae6a8d3)

## [0.5.8] - 2025-11-26

- Docs: publish the new CLAUDE guidance and move working references into docs/working for quicker updates. (da89924b)
- CLI: bump the npm package metadata to 0.5.8 so installs pick up the latest build. (44c59ddd)

## [0.5.7] - 2025-11-26

- Core/Exec: decode shell output with detected encodings so Unicode logs stay readable across platforms. (4aaba9b1, ae000766)
- Auto Drive: force read-only agents when no git repo exists to avoid accidental writes during automation. (13226204)
- App Server: emit token usage, compaction, and turn diff events plus thread metadata to improve monitoring. (401f94ca, caf2749d, 9ba27cfa, 157a16ce)
- Shell MCP: declare capabilities, add login support, and publish the npm package to keep tool integrations healthy. (c6f68c9d, e8ef6d3c, af63e6ec)

## [0.5.5] - 2025-11-25

- Auto Drive: restore 600-char CLI prompts, enforce sane bounds, add fallback to the current binary, and append test guidance to each goal for smoother automation handoffs. (24a0dd4c, 3651bc85, 15745f0a, f8f5c5b4)
- TUI/Prompts: add a full management section with save/reload, slash access, and alias autocomplete so custom prompts stay at your fingertips. (814fa485, 8d9e08c8, 079046e2)
- Streaming: show reconnecting spinners, log retry causes, and classify more transient errors so network hiccups stay visible without noise. (64a98b6b, 47e3cc76, 936cca8f)
- Agents: retier frontline options, upgrade opus/gemini defaults, and tighten descriptions to highlight the recommended models. (fa58356c, 58c83225, 884d008f)

## [0.5.4] - 2025-11-24

- Core/Agent: pass reasoning effort overrides through config so automation consistently honors requested budgets. (d6a7666b)
- Compact: trim chat history when context overflows and automatically retry to keep long sessions running. (2014c10d)

## [0.5.3] - 2025-11-24

- Auto Drive: add CLI aliases for automation runs and force headless sessions into full-auto so release flows stay hands-free. (288b1d94, 0bb2f8dd)
- TUI: keep tab characters intact during paste bursts and block stray Enter submits from per-key pastes for reliable composer input. (92625277, 019adc32)
- Connectivity: harden CLI/TUI retry paths so transient network drops automatically reconnect active sessions. (f0cb7afd, a7e4d25a)
- Config: honor CODE_HOME and CODEX_HOME entries from .env and retry without reasoning summaries when providers reject them. (5970ac52, 16ead0ec)

## [0.5.2] - 2025-11-22

- Agents: default automation flows to gpt-5.1-codex-max and add gemini-3-pro as an option for higher-capacity runs. (f0f99f2e)
- Models: clamp reasoning effort to supported bands so prompts no longer fail with invalid request errors. (6a7cac9d)

## [0.5.0] - 2025-11-21

- Rebrand the project to **Every Code** while keeping the `code` CLI name and refreshed docs.
- Auto Drive resilience: compaction and diagnostics, retry/backoff with observer telemetry, resume safety, and clearer cards/status.
- Default presets upgraded to gpt-5.1 with added codex-mini variants for lighter runs.
- UX polish: unified settings overlay refinements, /review uncommitted preset, strict streaming order, slash navigation hotkeys, and backtrack improvements.
- Notifications enabled by default plus clearer browser/exec logging and richer resume/session catalogs.
- Platform hardening: Nix offline builds, Windows AltGr + PATHEXT fixes, BSD keyring gating, responses proxy tightening, and sandbox/process safeguards.
- MCP & integrations: sturdier MCP client tooling, streamable HTTP support, improved Zed/ACP guidance, and a hardened responses API proxy.

## [0.4.21] - 2025-11-20

- Auto Drive: let runs choose a model and clamp verbosity so diagnostics stay predictable. (61209e0)
- Models: add gpt-5.1-codex-max default with one-time migration prompt so upgrades stay smooth. (8a97572, 6d67b8b, 64ae9aa)
- Core: wire execpolicy2 through core/exec-server and add shell fallbacks so commands keep running under the new policy. (65c13f1, 056c8f8, b00a7cf)
- TUI: add branch-aware filtering to `codex resume` so large workspaces find the right session faster. (526eb3f)
- Platform: enable remote compaction by default and schedule auto jobs to keep transcripts lean. (cac0a6a, 75f38f1)

## [0.4.20] - 2025-11-18

- Core: serialize `shell_command` tool invocations so concurrent steps no longer trample each other during runs. (497fb4a1)
- Models: ignore empty Claude `finish_reason` fields so streamed answers no longer truncate mid-response. (de1768d3)
- Windows: treat AltGr chords as literal text and resolve MCP script-based tools via PATHEXT so international keyboards and script servers work again. (702238f0, f828cd28)
- Core: overhaul compaction/truncation paths to remove double-truncation panics and keep summaries concise on long sessions. (94dfb211, 0b28e72b, 3f1c4b9a)
- Platform: gate keyring backends per target and add BSD hardening so FreeBSD/OpenBSD builds succeed out of the box. (5860481b)

## [0.4.19] - 2025-11-17

- Nix: vendor all git-sourced crates so offline builds no longer depend on network access. (079f833)
- Build: point the Nix derivation at the repo root to keep codex-rs workspace dependencies available. (079f833)

## [0.4.17] - 2025-11-17

- TUI: add an uncommitted preset to /review so you can diff local edits without staging. (dda8d2d)
- Resume: make the session picker async and add /push for fast handoff into publish. (a3be266)
- Resume: ignore system status snippets so regenerated plans stay focused on user messages. (e08999a)
- Resume: count user input coming from rollouts to keep token and action history accurate. (479edd1)
- Resume: unify the session catalog across views so saved sessions appear consistently. (0b26627)

## [0.4.16] - 2025-11-15

- TUI: enable desktop notifications by default so background job updates surface immediately. (799364de)
- TUI: refine unified exec with clearer UI and explicit workdir overrides for commands launched from history. (63c8c01f, f01f2ec9)
- Onboarding: handle "Don't Trust" directory selections gracefully so setup cannot get stuck in untrusted folders. (89ecc00b)
- SDK: add CLI environment override and AbortSignal support for better automation integrations. (93665000, 439bc5db)

## [0.4.15] - 2025-11-14

- Core: migrate default CLI, TUI, and Auto Drive models to gpt-5.1 so new sessions use the upgraded stack. (698c53f)
- Prompts: align the gpt-5.1 system instructions with Codex guidance to keep responses consistent. (e0ec79c)
- TUI Login: add device-code fallback and ensure ChatGPT auth links wrap cleanly on narrow terminals. (5279dd8, 2e47735, 322396c)

## [0.4.14] - 2025-11-13

- Settings: let reviewers choose the model used for /review from the settings overlay. (2134f3e)
- TUI: keep the final scrollback line visible after a command completes so transcripts stay readable. (f6f7a75)
- TUI: simplify the /merge handoff so follow-up flows resume without manual cleanup. (7d8684e)
- TUI: keep multiline slash commands intact when dispatching plan or solve sequences. (8d9398d)
- Stability: recover gracefully when the working directory vanishes mid-run instead of crashing. (97b956f)

## [0.4.11] - 2025-11-07

- Model: add gpt-5-codex-mini presets for quick access to lighter variants. (febfa7e)
- Compaction: add per-message summaries, checkpoint warnings, and prompt overrides to keep long transcripts clear. (b21190f, 8dd3c30, 58cf74d)
- Client: normalize retry-after handling, show resume times, and stop retrying fatal quota errors so recoveries are predictable. (0c82670, 0e0e85c, d996507)
- CLI: enable CTRL-n and CTRL-p to navigate slash commands, files, and history without leaving the keyboard. (e30f651)
- SDK: add network_access and web_search toggles to the TypeScript client for richer tool control. (c76528c)

## [0.4.9] - 2025-11-03

- CLI: rerun the bootstrap step when postinstall scripts are skipped so upgrades stay healthy. (8d842b8)
- Auto Drive: salvage user-turn JSON to keep transcripts recoverable after crashes. (38caf29)
- Homebrew: track the latest release so tap installs follow new versions immediately. (222d2e6)

## [0.4.8] - 2025-11-03

- Auto Drive: Surface coordinator schema details when retries fail so validation issues are actionable. (cbc31dad)
- TUI/Auto Drive: Resume the decision pipeline after diagnostics follow-ups so runs wrap up correctly. (1714bbd8)

## [0.4.7] - 2025-10-30

- TUI/Auto Drive: Keep router answers visible in the transcript so automation context stays complete. (cf17fc1)
- TUI/Auto Drive: Persist diagnostics follow-up prompts when resuming runs to avoid lost context. (d1e634d)

## [0.4.6] - 2025-10-30

- Build: keep release notes version in sync during `build-fast` to stop false release failures. (862851d)
- Build: drop the release notes gate so `build-fast` runs cleanly in CI. (b0bfd1f)
- CLI: publish the v0.4.6 package metadata for all platform bundles. (ceccd4b)

## [0.4.4] - 2025-10-29

- TUI: Interrupts in-flight runs when `/new` starts a fresh chat so responses never bleed between sessions. (0421f643)
- TUI/MCP: Keeps the selected MCP row visible while scrolling large server lists. (4c114758)
- Agents: Refreshes the Enabled toggle UX and persists state immediately in history. (56a0b37d)
- Config: Surfaces legacy `~/.codex/prompts` directories so custom prompts load automatically. (0be4f19c)
- Rollout: Sorts session history by latest activity to make resume picks faster. (9f6481a1)

## [0.4.2] - 2025-10-27

- Auto Drive: add compaction, token metrics, and durable transcripts so long runs stay stable. (0071313, 57b398f, cd880a5)
- TUI/Auto Drive: celebrate completion with a dedicated state to clarify run outcomes. (2130ed2)
- TUI: route browser status logs through the tail helper so log panes update live. (4630825)

## [0.4.1] - 2025-10-27

- Auto Drive: show in-progress summaries in the card so runs surface status while they execute. (c7991ed)
- Auto Drive: refresh gradients and status colors to clarify automation progress states. (ed0c895)
- TUI: restore the CLI send prompt label and stabilize vt100 rendering. (b1c04d0)
- Core/Debug: capture outgoing headers and order usage logs for clearer traces. (80497db)

## [0.4.0] - 2025-10-26

- Auto Drive: graduate the orchestrator into the new `code-auto-drive-core` crate, coordinate multi-agent runs end to end, and add self-checks plus elapsed-time tracking for every action.
- Automation workflow: rely on Auto Drive for long-lived `/auto` sessions — plan a run, hand it off, and return to completed work while the orchestrator pauses, resumes, and recovers automatically.
- Settings overlay: consolidate every `/settings` pane into a two-level overlay so limits, themes, and automation toggles sit in one place with quick navigation.
- TUI cards: introduce card-based history entries for Agents, Browser sessions, Web Search, and Auto Drive with grouped actions and overlays for deep detail.
- Performance: tighten CPU and memory usage discovered in heavy automation scenarios to keep scrolling and rendering smooth.
- Agents: let `/plan`, `/code`, and related commands target specific CLIs (e.g., `gemini-3-flash`, `claude-sonnet-4.5`) with future expansion handled from the new settings hub.

## [0.2.188] - 2025-10-06

- MCP: Validate stdio tool commands on PATH and surface clearer spawn errors during setup. (3a51d30)
- Release: Guard release notes generation so headers always match the published version. (db38a24)

## [0.2.187] - 2025-10-06

- TUI: Maintain strict streaming order and stable scrollback so history stays put while answers land. (554f2e6b)
- CLI: Prefer rollout `.jsonl` transcripts when resuming sessions so `code resume` stays reliable after snapshots. (7f69aa55)
- Core/Auth: Automatically use stored API keys for enterprise ChatGPT plans and honor retry hints from rate-limit errors. (fa1bd81f)

## [0.2.184] - 2025-10-03

- Core: Record pid alongside port in server info to simplify local process debugging. (3778659)
- CLI: Support CODEX_API_KEY in `codex exec` so credentials can be set via environment. (2f6fb37)
- TUI: Make the model switcher a two-stage flow to prevent accidental model swaps. (06e34d4)
- TUI: Surface live context window usage while tasks run to clarify token budgets. (2f370e9)
- TUI: Show a placeholder when commands produce no output to keep history legible. (751b3b5)

## [0.2.183] - 2025-10-03

- TUI/Explore: keep the Exploring header until the next non-reasoning entry to maintain exploration context. (7048793)
- TUI/Explore: sync reasoning visibility changes with explore cells to avoid stale header state. (7048793)

## [0.2.182] - 2025-10-03

- Agents: add first-class cloud agent flows with submit --wait support and richer previews. (6556ac9, 51e60cb, cdc833d)
- Protocol: introduce MCP shim and sync the tool stack for new runtime integrations. (d0a0f01)
- Auto-drive: forward CLI context, export rendered history, and surface progress for coordinator runs. (0d2e51f, ec0bbc4, bccd39f)
- TUI: open the undo timeline overlay on double Esc and persist auto-resolve workflows between sessions. (a658b1a, e1916f7, 4e35c09)

## [0.2.180] - 2025-10-01

- Auto-drive: add final observer validation before finishing runs so missed work is caught. (94061ee)
- Auto-drive: stop appending ellipsis to decision summaries so prompts stay clean. (6384601)
- TUI/History: completed state-driven refactor; all cells render from `HistoryState` via the shared renderer cache with stable IDs and domain events. (local)
- Docs: captured the history architecture in `docs/tui-chatwidget-refactor.md`, `docs/history_state_schema.md`, and `docs/history_render_cache_bridge.md`. (local)

## [0.2.179] - 2025-09-30

- Auto-drive: persist conversation between turns and retain the raw coordinator transcript so context carries forward. (6d6e3f5, 6edced3)
- Auto-drive: restore streaming reasoning titles and tidy decision summaries by removing stray ellipses. (104febe, 001a415, 25b6d0c)
- Auto-drive: surface spinner status when the composer is hidden, show progress in the title, and refresh the footer CTA styling. (935876a, eb54fe6, 86600f8, c9550bd)
- Auto-drive: expand coordinator guidance with AUTO_AGENTS instructions to keep automation setups aligned. (2029365)
- TUI/Theme: reuse a shared RGB mapping for ANSI fallbacks to make colors consistent across terminals. (a904665, 258e032)

## [0.2.178] - 2025-09-30

- Auto-drive: restructure coordinator transcript to clarify CLI roles and context. (4733856)
- Auto-drive: show coordinator summary while CLI commands execute so guidance stays visible. (5e273dc)
- Auto-drive: require mandatory observer fields to avoid partial telemetry updates. (962e482)
- TUI: round rate-limit windows with local reset times for accurate throttling feedback. (0b13c26)
- TUI/Theme: preserve assistant tint across palettes to keep colors consistent across terminals. (87817ae)

## [0.2.177] - 2025-09-29

- Core/CLI: centralize pre-main process hardening into `codex-process-hardening` and invoke it automatically when secure mode is enabled. (bacba3f)
- CLI/Proxy: rename the responses proxy binary to `codex-responses-api-proxy`, harden startup, and remove request timeouts so streaming stays reliable. (bacba3f)
- Auto-drive: relay plan updates to the coordinator so guidance stays aligned with the latest steps. (b7a8d7f)
- TUI/Auto-drive: show the waiting spinner only while the coordinator is active to avoid idle animation churn. (9e622ab)

## [0.2.176] - 2025-09-29

- Auto-drive: add an observer thread and telemetry stream to watch automation health in real time. (fd7d3a71)
- TUI/Update: harden guided upgrade flows to recover cleanly from partial runs. (4a4b70dd)
- TUI/Theme: introduce dedicated 16-color palettes so limited terminals render accurately. (5efc9e0a)
- TUI: trim fallback prompt copy and reset upgrade flags after completion. (79f02a18)

## [0.2.175] - 2025-09-29

- TUI/Auto-drive: add retry/backoff orchestration so coordinator runs recover after transient failures. (6c5856e8)
- TUI/Auto-drive: honor rate-limit reset hints and jittered buffers to resume safely after 429 responses. (6c5856e8)
- Docs: outline dev fault injection knobs for rehearsing auto-drive failure scenarios. (6c5856e8)

## [0.2.174] - 2025-09-29

- TUI/Auto-drive: gate weave animations to the active phase so idle sessions stay calm. (70f64ae8)
- TUI/Auto: require full auto mode and show env context to clarify coordinator state. (599b886c)

## [0.2.173] - 2025-09-29

- TUI/Browser: auto hand off /browser startup failures to Code so sessions self-heal. (95e27cd0)
- TUI/Browser: sanitize and surface error details when handoff triggers for faster diagnosis. (95e27cd0)

## [0.2.172] - 2025-09-28

- CLI: introduce a responses API proxy command so shared hosts can forward Responses calls securely. (c5494815)
- MCP: add streamable HTTP client support and tighten per-call timeout handling. (3a1be084, bd6aad0b, d3ecf4de, 9bb84892)
- Auto-drive: stream coordinator reasoning, keep plan context, and smooth heading presentation. (fc8da8cf, 0d5bc1f1, 45a6fb94, aaa91865)
- TUI/History: route diff and explore cells through domain events for consistent playback. (0ae94dca, 0d5d57ee)

## [0.2.170] - 2025-09-27

- TUI/History: drive exec, assistant, explore, and rate-limit cells from domain events for consistent streaming. (38abccc, 5409493, e6e72d3, f851369, d7e9ee7)
- TUI/Notifications: add an OSC toggle command and harden slash routing, persistence, and filters so alerts stay accurate. (6a8aac2, 7ab484a, 449aeb0, 9c4101c, 51a3c28)
- Usage/Rate-limits: compact persisted stats, relog after resets, and persist reset state to keep quotas current. (7573b40, bf325f7, 51fd601)
- TUI/Accounts: prioritize ChatGPT accounts in login flows and restore the label prefix for clarity. (1db6193, 489bfb1)
- UX: show a session resume hint on exit, surface Zed model selection, and restore Option+Enter newline plus Cmd+Z undo. (13dd4ca, 0e39d1f, eed1952)

## [0.2.168] - 2025-09-26

- TUI/Limits: restore the 6 month history view with expanded layout, spacing, and weekday labels. (840998b, 7c956be, 2f6b506, 395e2ba)
- TUI/History: persist assistant stream records so prior reasoning stays available after reloads. (fbe7528)
- TUI/Worktrees: stop deleting other PID checkouts and clean up stale directories after EINTR interruptions. (4971783, da2840b)
- TUI/Chat: keep manual file search queries synced for repeat lookups. (883b8f2)
- CLI: adopt the pre-main hardening hook to align with tighter runtime protections. (47b7de3)

## [0.2.167] - 2025-09-26

- TUI/Terminal: allow blank dollar prompt to open a shell instantly. (e8ed566)
- TUI/Status: rebuild /status with card layout and richer reasoning context. (e363dac)
- TUI/Limits: persist rate-limit warning logs across sessions so spikes stay visible. (3617541)
- Core/Compact: store inline auto-compaction history to stabilize collapsed output. (5b3c7b5)
- TUI/Input: restore Warp.dev command+option editing for smoother text adjustments. (2fa3f47)

## [0.2.166] - 2025-09-25

- TUI/History: refresh the popular commands lineup so quick actions match current workflows. (03af55d1)
- TUI/Auto-upgrade: silence installer chatter and log completion once updates finish. (f23e920c)
- Core/Client: skip the web_search tool when reasoning is minimal to reduce latency. (75314d7e)
- TUI/Input: normalize legacy key press/release cases so hotkeys stay consistent on older terminals. (0415dd9f, 06eeac5a, 64922dfb)
- Nix: make codex-rs the default package and drop the broken codex-cli derivation. (fa945b5f, 9ec8f3eb)

## [0.2.165] - 2025-09-25

- TUI/Theme: cache terminal background detection and skip OSC probe when theme is explicit. (eeefdf5, 166fa57)
- Agents: clear idle spinner and avoid empty task preview text in chat. (499f14b)
- Workflows: escape issue titles in PR fallback for issue-code automation. (67a9882)
- MCP Server: use codex_mcp_server imports for bundled tooling compatibility. (f846c9d)

## [0.2.164] - 2025-09-25

- TUI/Limits: track API reset timers across core and TUI so rate windows stay accurate. (417c1867)
- CLI/Postinstall: restore shim detector and avoid overwriting existing code shim so installs stay intact. (ff63c4d0, 480640fa, ce317cbf)
- Core/Config: allow overriding OpenAI wire API and support OpenRouter routing metadata for custom deployments. (a49cd2cd, 060cd5e2)
- Core/Agents: cap agent previews and handle updated truncation tuple to stay within API limits. (a52dd361, c47f6767)

## [0.2.162] - 2025-09-22

- CLI/Resume: fix --last to reliably select the most recent session under active runtimes. (1a2521ff)
- Stability: avoid nested Tokio runtime creation during resume lookup to prevent sporadic failures. (1a2521ff)

## [0.2.161] - 2025-09-22

- TUI/Slash: add more /review options for richer reviews (5996ee0e)
- TUI: fix merge fallout and remove unused const to eliminate warnings (cf031b67)

## [0.2.160] - 2025-09-22

- Core/Config: coerce auto-upgrade booleans for correct behavior (158cb551)
- TUI/Slash: route exit aliases through quit for consistency (f527c1d1)
- Core/Exec: block redundant cd and Python file writer commands (e8c78311)

## [0.2.159] - 2025-09-22

- Docs: streamline Zed integration guide (ceb8804c)
- No user-facing code changes; release metadata only (2e77ed94, 323b6563)

## [0.2.158] - 2025-09-22

- Core/ACP: integrate ACP support and sync protocol updates (1eeae7f8, fbe8beb5)
- CLI: expose MCP via code subcommand and add acp alias; ship code-mcp-server on install (f15d2e2f, d41e9064, 33f498e1)
- TUI/Limits: refresh layout, show compact usage, align hourly/weekly windows (20aaecb3, 06bcddfd, fecaf661)
- TUI/Limits: fix hourly window display and reset timing (388975ac)
- Stability: respect web search flag; clear spinner after final answer; accept numeric MCP protocolVersion (c5dfc88d, d95c24b1, 763e08c5)

## [0.2.157] - 2025-09-22

- CLI: restore coder resume support. (b46786d3)
- CLI: generate completion scripts with code command name. (b8961ec0)
- TUI: avoid showing agents HUD on handoff. (8ab7367c)
- TUI/Limits: refresh usage header copy; move rate‑limit polling off main thread. (ddc23f6a, 2d3a9f55)
- TUI: show Ctrl+T exit hint in standard mode. (6243cf27)

## [0.2.156] - 2025-09-22

- TUI/Limits: add /limits view with live snapshots and persisted reset times. (70ee0986, a9fc289a, b2567a0e)
- Performance: speed up exec/history rendering via layout and metadata caching. (50399898, fbf6a9d6, 9f2b39a0)
- Approval: require confirmation for manual terminal commands; add semantic prefix matching. (d9be45a8, 57ecce7c)
- Core: report OS and tool info for better diagnostics. (5142926c)
- TUI/History: show run duration, collapse wait tool output, and finalize cells cleanly. (8cdba3a6, 3aa2e17a, 5378e55d)

## [0.2.155] - 2025-09-18

- Auth: fix onboarding auth prompt gating. (87a76d25)
- CLI: add long-run calculator script. (b01e2b38)
- TUI: add pulldown-cmark dependency to fix build. (f1718b03)
- Docs: clarify config directories. (cc22fbd9)

## [0.2.154] - 2025-09-18

- TUI/Input: fix Shift+Tab crash. (354a6faa)
- TUI/Agents: improve visibility for multi‑agent commands. (8add2c42)
- TUI/Slash: make @ shortcut work with /solve and /plan. (db324a6c)

## [0.2.153] - 2025-09-18

- Core/Config: prioritize ~/.code for legacy config reads and writes. (d268969, 2629790)
- TUI/History: strip sed/head/tail pipes when showing line ranges. (d1880bb)
- TUI: skip alternate scroll on Apple Terminal for smoother scrolling. (f712474)
- Resume: restore full history replay. (6d1bfdd)
- Core: persist GPT-5 overrides across sessions. (26e538a)

## [0.2.152] - 2025-09-17

- TUI: add terminal overlay and agent install flow. (104a5f9f, c678d670)
- TUI/Explore: enrich run summaries with pipeline context; polish explore labels. (e25f8faa, d7ce1345)
- Core/Exec: enforce dry-run guard for formatter commands. (360fbf94)
- Explore: support read-only git commands. (b6b9fc41)
- TUI: add plan names and sync terminal title. (29eda799)

## [0.2.151] - 2025-09-16

- TUI/History: append merge completion banner for clearer post-merge status. (736293a9)
- TUI: add intersection checks for parameter inputs in AgentEditorView. (6d1775cf)

## [0.2.150] - 2025-09-16

- TUI/Branch: add /merge command and show diff summary in merge handoff. (eb4c2bc0, 0f254d9e, b19b2d16)
- TUI/Agents: refine editor UX and persistence; keep instructions/buttons visible and tidy spacing. (639fe9dd, f8e51fb9, 508e187f)
- TUI/History: render exec status separately, keep gutter icon, and refine short-command and path labels. (2ec5e655, fd8f7258, 59975907, a27f3aab)
- Core/TUI: restore jq search and alt-screen scrolling; treat jq filters as searches. (8c250e46, ec1f12cb, 764cd276)

## [0.2.149] - 2025-09-16

- TUI/Agents: redesign editor and list; keep Save/Cancel visible, add Delete, better navigation and scrolling. (eb024bee, 8c2caf76, 647fed36)
- TUI/Model: restore /model selector and presets; persist model defaults; default local agent is "code". (84fbdda1, 85159d1f, 60408ab1)
- TUI/Reasoning: show reasoning level in header; keep reasoning cell visible; polish run cells and log claims. (d7d9d96d, 2f471aee, 8efe4723)
- Exec/Resume: detect absolute bash and flag risky paths; fix race in unified exec; show abort and header when resuming. (4744c220, d555b684, 50262a44, 6581da9b)
- UX: skip animations on small terminals, update splash, and refine onboarding messaging. (934d7289, 9baa5c33, 5c583fe8)

## [0.2.148] - 2025-09-14

- Core/Agents: mirror Qwen/DashScope API vars; respect QWEN_MODEL; add qwen examples in config.toml.example. (8a935c18)
- Shortcuts: set Qwen-coder as default for /plan and related commands. (d1272d5e)

## [0.2.147] - 2025-09-14

- Core/Git Worktree: add opt-in mirroring of modified submodule pointers via CODEX_BRANCH_INCLUDE_SUBMODULES. (59a6107d)
- Core/Git: keep default behavior unchanged to avoid unexpected submodule pointer updates. (59a6107d)

## [0.2.146] - 2025-09-14

- TUI: rewrite web.run citation tokens into inline markdown links. (66dbc5f2)
- Core: fix /new to fully reset chat context. (d4aee996)
- Core: handle sandboxed agent spawn when program missing. (5417eb26)
- Workflows: thread issue comments; show digests oldest→newest in triage. (e63f5fc3)

## [0.2.145] - 2025-09-13

- CI/Issue comments: ensure proxy script is checked out in both jobs; align with upstream flows. (81660396)
- CI: gate issue-comment job on OPENAI_API_KEY via env and avoid secrets in if conditions. (c65cf3be)

## [0.2.144] - 2025-09-13

- CI/Issue comments: make agent assertion non-fatal; fail only on proxy 5xx; keep fallback path working. (51479121)
- CI: gate agent runs on OPENAI key; fix secrets condition syntax; reduce noisy stream errors; add proxy log tail for debug. (31a8b220, 3d805551, b94e2731)

## [0.2.143] - 2025-09-13

- Core: fix Responses API 400 by using supported 'web_search' tool id. (a00308b3)
- CI: improve slug detection and labeling across issue comments and previews. (98fa99f2, 1373c4ab)
- CI: guard 'Codex' branding regressions and auto-fix in TUI/CLI. (f20aee34)

## [0.2.142] - 2025-09-12

- CI: avoid placeholder-only issue comments to reduce noise. (8254d2da)
- CI: gate Code generation on OPENAI_API_KEY; skip gracefully when missing. (8254d2da)
- CI: ensure proxy step runs reliably in workflows. (8254d2da)

## [0.2.141] - 2025-09-12

- Exec: allow suppressing per‑turn diff output via `CODE_SUPPRESS_TURN_DIFF` to reduce noise. (ad1baf1f)
- CI: speed up issue‑code jobs with cached ripgrep/jq and add guards for protected paths and PR runtime. (ad1baf1f)

## [0.2.140] - 2025-09-12

- No user-facing changes; maintenance-only release with CI cache prewarming and policy hardening. (1df29a6f, 6f956990, fa505fb7)
- CI: prewarm Rust build cache via ./build-fast.sh to speed upstream-merge and issue-code agents. (6f956990, fa505fb7)
- CI: align cache home with enforced CARGO_HOME and enable cache-on-failure for more reliable runs. (1df29a6f)

## [0.2.139] - 2025-09-12

- TUI/Spinner: set generation reasoning effort to Medium to improve quality and avoid earlier Minimal/Low issues. (beee09fc)
- Stability: scope change to spinner-generation JSON-schema turn only; main turns remain unchanged. (beee09fc)

## [0.2.138] - 2025-09-12

- TUI/Spinner: honor active auth (ChatGPT vs API key) for custom spinner generation to avoid 401s. (e3f313b7)
- Auth: prevent background AuthManager resets and align request shape with harness to stop retry loops. (e3f313b7)
- Stability: reduce spinner‑creation failures by matching session auth preferences. (e3f313b7)

## [0.2.137] - 2025-09-12

- Proxy: default Responses v1; fail-fast on 5xx; add STRICT_HEADERS and RESPONSES_BETA override. (acfaeb7d, 1ddedb8b)

## [0.2.133] - 2025-09-12

- Release/Homebrew: compute `sha256` from local artifacts; add retry/backoff when fetching remote bottles; avoid failing during CDN propagation. (fd38d777b)
- CI/Triage: remove OpenAI proxy and Rust/Code caches; call API directly in safety screen to simplify and speed up runs. (7a28af813)
- Dev: add `scripts/openai-proxy.js` for local testing with SSE‑safe header handling; mirrors CI proxy behavior. (7e9203c22)

## [0.2.132] - 2025-09-12

- CI/Upstream‑merge: verbose OpenAI proxy with streaming‑safe pass‑through and rich JSON logs; upload/tail logs for diagnosis. (43e6afe2d)
- CI/Resilience: add chat‑completions fallback provider; keep Responses API as default; prevent concurrency cancellation on upstream‑merge. (3d4687f1b, e27f320e6)
- CI/Quality gate: fail job on server/proxy errors seen in agent logs to avoid silent successes. (62695b1e5)

## [0.2.131] - 2025-09-12

- Core/HTTP: set explicit `Host` header from target URL to fix TLS SNI failures when using HTTP(S)_PROXY with Responses streaming. (6ad9cb283)
- Exec/Workflows: exit non‑zero on agent Error events so CI fails fast on real stream failures. (fec6aa0f0)
- Proxy: harden TLS forwarding (servername, Host reset, hop‑by‑hop header cleanup). (fec6aa0f0)

## [0.2.130] - 2025-09-12

- Core/Client errors: surface rich server context on final retry (HTTP status, request‑id, body excerpt) instead of generic 500s; improve UI diagnostics. (6be233187)
- Upstream sync: include `SetDefaultModel` JSON‑RPC and `reasoning_effort` in `NewConversationResponse`. (35bc0cd43, 9bbeb7536)

## [0.2.129] - 2025-09-12

- TUI/Spinner: hide spinner after agents complete; refine gating logic. (08bdfc46e)
- TUI/Theme: allow Left/Right to mirror Up/Down; enable Save/Retry navigation via arrows in review forms. (2994466b7)

## [0.2.128] - 2025-09-11

- Upstream: onboarding experience, usage‑limit CTA polish, MCP docs, sandbox timeout improvements, and lint updates. (8453915e0, 44587c244, 8f7b22b65, 027944c64, bec51f6c0, 66967500b)

## [0.2.127] - 2025-09-11

- MCP: honor per‑server `startup_timeout_ms`; make `tools/list` failures non‑fatal; add test MCP server and smoke harness to validate slow/fast cases. (f69ea8b52)

## [0.2.126] - 2025-09-11

- TUI/Branch: preserve chat history when switching with `/branch`; finalize at repo root to avoid checkout errors. (0ae8848bd)

## [0.2.125] - 2025-09-11

- Windows CLI: stop appending a second `.exe` in cache/platform paths; use exact target triple. (a674e40e5)

## [0.2.124] - 2025-09-11

- Windows bootstrap: robust unzip in runtime bootstrap (PowerShell full‑path, `pwsh`, `tar` fallback); extract to user cache. (1a31d2e1a)

## [0.2.123] - 2025-09-11

- Upstream merge: reconcile with `openai/codex@main` while restoring fork features and keeping local CLI/TUI improvements. (742ddc152, a0de41bac)
- Windows bootstrap: always print bootstrap error; remove debug gate. (74785d58b)

## [0.2.122] - 2025-09-11

- Agents: expand context to include fork enhancements for richer prompts. (7961c09a)
- Core: add generic guards to improve stability during upstream merges. (7961c09a)

## [0.2.121] - 2025-09-11

- CLI: make coder.js pure ESM; replace internal require() with fs ESM APIs. (a5da604e)
- CLI: avoid require in isWSL() to prevent CJS issues under ESM. (a5da604e)

## [0.2.120] - 2025-09-11

- CLI/Install: harden Windows and WSL install paths to avoid misplacement. (9faf876c)
- CLI/Install: improve file locking to reduce conflicts during upgrade. (9faf876c)

## [0.2.119] - 2025-09-11

- CLI/Windows: fix global upgrade failures (EBUSY/EPERM) by caching the native binary per-user and preferring the cached launcher. (faa712d3)
- Installer: on Windows, install binary to %LocalAppData%\just-every\code\<version>; avoid leaving a copy in node_modules. (faa712d3)
- Launcher: prefer running from cache; mirror into node_modules only on Unix for smoother upgrades. (faa712d3)

## [0.2.118] - 2025-09-11

- TUI/Theme: add AI-powered custom theme creation with live preview, named themes, and save without switching. (a59fba92, eb8ca975, abafe432, 4d9335a3)
- Theme Create: stream reasoning/output for live UI; salvage first JSON object; show clear errors with raw output for debugging. (53cc6f7b, 353c4ffc, 85287b9e, e49ecb1a)
- Theme Persist: apply custom colors only when using Custom; clear colors/label when switching to built-ins. (69e6cc16)
- TUI: improve readability and input — high-contrast loading/input text; accept Shift-modified characters. (1f6ca898, fe918517)
- TUI: capitalize Overview labels; adjust "[ Close ]" spacing and navigation/height. (b7269b44)

## [0.2.117] - 2025-09-10

- TUI: route terminal paste to active bottom-pane views; enable paste into Create Spinner prompt. (a48ad2a1)
- TUI/Spinner: balance Create preview spacing; adjust border width and message text. (998d3db9)

## [0.2.116] - 2025-09-10

- TUI: AI-driven custom spinner generator with live streaming, JSON schema, and preview. (d7728375)
- Spinner: accept "name" in custom JSON; persist label; show labels in Overview; replace on save. (704286d3)
- TUI: dim "Create your own…" until selected; use primary + bold on selection. (09685ea5)
- TUI: fix Create Spinner spacing; avoid double blank lines; keep single spacer. (7fe209a0)
- Core: add TextFormat and include text.format in requests. (d7728375)

## [0.2.115] - 2025-09-10

- TUI/Status: keep spinner visible during transient stream errors; show 'Reconnecting' instead of clearing. (56d7784f)
- TUI/Status: treat retry/disconnect errors as background notices rather than fatal failures. (56d7784f)

## [0.2.114] - 2025-09-10

- TUI: honor custom spinner selection by name; treat as current. (a806d640)
- TUI: show custom spinner immediately and return to Overview on save. (a806d640)

## [0.2.113] - 2025-09-10

- TUI: improve Create Custom spinner UX with focused fields, keyboard navigation, and clear Save/Cancel flow; activating saved spinner immediately. (08a2f0ee)
- TUI: refine spinner list spacing and borders; dim non-selected rows for clearer focus. (a6009916, 7e865ac9)
- Build: fix preview release slug resolution from code/<slug> with fallbacks. (722af737)

## [0.2.112] - 2025-09-10

- TUI: group spinner list with dim headers and restore selector arrow for clearer navigation. (085fe5f3)
- Repo: adopt code/<slug> label prefix with id/ fallback across workflows. (dff60022)
- Triage: add allow/block/building/complete labels and use label as SSOT for slug in workflows. (17cc1dc6)

## [0.2.111] - 2025-09-10

- Automation: include issue body, recent comments, and commit links in context; expand directly in prompt (b3a1a65b)
- Automation: pick last non-placeholder comment block to avoid stale summaries (e18f1cbd)

## [0.2.110] - 2025-09-10

- Automation: update issue comments — remove direct download links, add LLM template and user mentions; keep commit summary (546b0a4e)
- Triage: defer user messaging to issue-comment workflow; remove queue acknowledgement (5426a2eb)
- TUI: remove unused imports to silence build warnings (ed6b4995)

## [0.2.109] - 2025-09-10

- TUI: improve spinner selection (exact/case-insensitive), center previews, restore overview values (51422121)
- Automation: issue comments include recent commit summaries; ignore placeholders and fall back to stock summary with commits/files (9915fa03, f62b7987)

## [0.2.108] - 2025-09-10

- TUI: Add /theme Overview→Detail flow with live previews for Theme and Spinner selection. (535d0a9c)
- TUI: Bundle full cli-spinners set and allow choosing your loading spinner; 'diamond' stays default. (990b07a6, 247bb19c)
- TUI: Improve scrolling with anchored 9-row viewport; keep selector visible and dark-theme friendly. (ad859a33, 8deb7afc)
- Core: Split stdout/stderr in Exec output and add ERROR divider on failures for clarity. (dff216ec)

## [0.2.107] - 2025-09-09

- Core: Fix planning crash on UTF-8 boundary when previewing streamed text. (daa76709)
- Stability: Use char-safe slicing for last 800 chars to prevent panics. (daa76709)

## [0.2.106] - 2025-09-09

- CLI/Preview: save downloads under ~/.code/bin by default; suffix binaries with PR id. (3bebc2d1)
- CLI/Preview: run preview binary directly (no --help) for simpler testing. (36cfabfa)
- Preview build: use gh -R and upload only files; avoid .git dependency. (1b3da3b3)

## [0.2.105] - 2025-09-09

- Triage: make agent failures non-fatal; capture exit code and disable git prompts. (adbcfbae)
- Triage: forbid agent git commits; treat agent-made commits as changes; allow branch/push even when clean. (11f7adcb)
- Preview: fix code-fence array string and YAML error to restore builds. (7522c49f)

## [0.2.104] - 2025-09-09

- CLI: support preview downloads via pr:<number>; keep run-id fallback. (73de54da)
- Preview: publish prereleases on PRs with release assets; no-auth downloads. (73de54da)
- PR comment: recommend 'code preview pr:<number>' for clarity. (73de54da)

## [0.2.103] - 2025-09-09

- Build: add STRICT_CARGO_HOME to enforce CARGO_HOME; default stays repo-local when unset. (6cbc0555)
- Triage/Agent: standardize CARGO_HOME and share with rust-cache; prevent env overrides and unintended cargo updates. (13ffc850)
- CI/Upstream-merge: fix YAML quoting and no-op outputs; split precheck and gate heavy work at job level for reliability. (a1526626, a9bb2b6a)

## [0.2.102] - 2025-09-09

- CI/Triage: fetch remote before push and fall back to force-with-lease on non-fast-forward for bot-owned branches. (f4258aeb, 81dac6d6)
- Agents: pre-create writable CARGO_HOME and target dirs for agent runs to avoid permission errors. (0ad69c90)

## [0.2.101] - 2025-09-09

- Build: remove OpenSSL by using rustls in codex-ollama; fix macOS whoami scope. (c3034c38)
- Core: restore API re-exports and resolve visibility warning. (b29212ca)
- TUI: Ctrl+C clears non-empty prompts. (58d77ca4)
- TUI: paste with Ctrl+V checks file_list. (1f4f9cde)
- MCP: add per-server startup timeout. (6efb52e5)

## [0.2.100] - 2025-09-09

- Core: fix date parsing in rollout preflight to compile. (6eec307f)
- Build: speed up build-fast via sccache; keep env passthrough for agents. (ff4b0160)
- Release: add preflight E2E tests and post-build smoke checks to improve publish reliability. (a97b8460, 6c09ac42)
- Upstream-merge: refine branding guard to check only user-facing strings. (da7581de)

## [0.2.99] - 2025-09-09

- TUI/Branch: finalize merges default into worktree first; prefer fast-forward; start agent on conflicts. (8e1cbd20)
- TUI/History: cache Exec wrap counts and precompute PatchSummary layout per width to reduce measurement. (be3154b9)

## [0.2.98] - 2025-09-09

- TUI/Footer: restore 0.2.96 behavior; remove duplicate Access flash; add Shift+Tab to Help; make 'Full Access' label ephemeral. (8e4c96de)
- TUI/Footer: fix ephemeral 'Full Access' label on Shift+Tab so it doesn't clear immediately. (062b83d7)
- TUI/Footer: reapply DIM styling so footer text is visibly dimmer (matches 0.2.96). (78b3d998)
- TUI/Footer: remove bold from access label and add a leading space for padding. (4e8bece8, 950fbacf)

## [0.2.97] - 2025-09-08

- CI/Preview: add PR preview builds for faster review. (cd624877)
- Workflows/Triage: add triage‑first agent to prioritize issues. (cd624877)
- TUI: show richer comments in PR previews. (cd624877)

## [0.2.96] - 2025-09-08

- Core/Auth: prefer ChatGPT over API key when tokens exist. (a8cd8abd)
- CI/Upstream-merge: strengthen ancestor checks, gate mirroring on reason, show skip_reason. (55909c25)

## [0.2.95] - 2025-09-08

- TUI: guard xterm focus tracking on Windows/MSYS and fragile terminals. (9e535afb)
- TUI: add env toggles to control terminal focus tracking behavior. (9e535afb)

## [0.2.94] - 2025-09-08

- TUI: add footer access‑mode indicator; Shift+Tab cycles Read Only / Approval / Full Access. (0a34e912)
- TUI: show access‑mode status as a background event early; update Help with shortcut. (0a34e912)
- Core: persist per‑project access mode in config.toml and apply on startup. (0a34e912)
- Core: clarify read‑only write denials and block writes immediately in RO mode. (0a34e912)

## [0.2.93] - 2025-09-08

- TUI/Core: show Popular commands on start; track and clean worktrees. (2908be45)
- TUI/MCP: add interactive /mcp settings popup with on/off toggles; composer prefill. (5e9ce801, 7456b3f0)
- TUI/Onboarding: fix stray import token causing build failure. (707c43c2)
- TUI/Branch: fix finalize pattern errors under Rust 2024 ergonomics. (54659509)

## [0.2.92] - 2025-09-08

- Core/Git Worktree: create agent worktrees under ~/.code/working/<repo>/branches for isolation. (e9ebcf1f)
- Core/Agent: sandbox non-read-only agent runs to worktree to prevent writes outside branch. (ad2f141e)

## [0.2.91] - 2025-09-08

- TUI/Panic: restore terminal state and exit cleanly on any thread panic. (34ffe467)
- TUI/Windows: prevent broken raw mode/alt-screen after background panics under heavy load. (34ffe467)

## [0.2.90] - 2025-09-08

- TUI/History: Home/End jump to start/end when input is empty. (7287fa71, 60f9db8c)
- TUI/Overlays: Esc closes Help/Diff; hide input cursor while active. (d7353069)
- TUI/Help: include Slash Commands; left-align keys; simplify delete shortcuts. (e00a4ecd, 11a7022d, 25aa36a3)
- TUI: rebrand help and slash descriptions to "Code"; hide internal /test-approval. (5a93aee6, bde3e624)

## [0.2.89] - 2025-09-08

- TUI/Help: add Ctrl+H help overlay with key summary; update footer hint. (c1b265f8)
- TUI/Input: add Ctrl+Z undo in composer and route it to Chat correctly. (a589aeee, 0cbeb651)
- TUI/Input: map Ctrl+Backspace to delete the current line in composer. (c422d92d)
- TUI/Branch: treat "nothing to commit" as success on finalize and continue cleanup. (e9d2a246)

## [0.2.88] - 2025-09-08

- Core/Git: ensure 'origin' exists in new worktrees and set origin/HEAD for default branch to improve git UX. (c59fd7e2)
- TUI/Footer: show one-time Shift+Up/Down history hint on first scroll. (9a4bddc7)
- TUI/Input: support macOS Command-key shortcuts in the composer. (7f021e37)
- TUI/Branch: add hidden preface for auto-submitted confirm/merge-and-cleanup flow; prefix with '[branch created]' for clarity. (16b78005, a78a2256)

## [0.2.87] - 2025-09-08

- TUI/History: make Shift+Up/Down navigate history in all popups; persist UI-only slash commands to history. (16c38b6b)
- TUI/Branch: preserve visibility by emitting 'Switched to worktree: <path>' after session swap; avoid losing the confirmation message on reset. (5970a977)
- TUI/Branch: use BackgroundEvent for all /branch status and errors; retry with a unique name if the branch exists; propagate effective branch to callers. (40783f51)
- TUI/Branch: split multi-line worktree message into proper lines for clarity. (959a86e8)

## [0.2.86] - 2025-09-08

- TUI: add `/branch` to create worktrees, switch sessions, and finalize merges. (8f888de1)
- Core: treat only exit 126 as sandbox denial to avoid false escalations. (e4e5fb01)
- Docs: add comprehensive slash command reference and link from README. (a3b5c18a)

## [0.2.85] - 2025-09-07

- TUI: insert plan/background events near-time and keep reasoning ellipsis during streaming. (81a31dd5)
- TUI: approvals cancel immediately on deny and use a FIFO queue. (0930b6b0)
- Core: fix web search event ordering by stamping OrderMeta for in-turn placement. (81a31dd5)

## [0.2.84] - 2025-09-07

- Core: move token usage/context accounting to session level for accurate per‑session totals. (02690962)
- Release: create_github_release accepts either --publish-alpha or --publish-release to avoid conflicting flags. (70a6d4b1)
- Release: switch tooling to use gh, fresh temp clone, and Python rewrite for reliability. (b1d5f7c0, 066c6cce, bd65f81e)
- Repo: remove upstream‑only workflows and TUI files to align with fork policy. (e6c7b188)

## [0.2.83] - 2025-09-07

- TUI: theme-aware JSON preview in Exec output; use UI-matched highlighting and avoid white backgrounds. (ac328824)
- TUI: apply UI-themed JSON highlighting for stdout; clear ANSI backgrounds so output inherits theme. (722fb439)
- Core: replace fragile tree-sitter query with a heredoc scanner in embedded apply_patch to prevent panics. (00ffb316)

## [0.2.81] - 2025-09-07

- CI: run TUI invariants guard only on TUI changes and downgrade to warnings to reduce false failures. (d41da1d1, 53558af0)
- CI: upstream-merge workflow hardens context prep; handle no merge-base and forbid unrelated histories. (e410f2ab, 8ee54b85)
- CI: faster, safer fetch and tools — commit-graph/blobless fetch, cached ripgrep/jq, skip tag fetch to avoid clobbers. (8ee54b85, 23f1084e, dd0dc88f)
- CI: improve reliability — cache Cargo registry, guard apt installs, upload .github/auto artifacts and ignore in git; fix DEFAULT_BRANCH. (e991e468, ee32f3b8, b6f6d812)

## [0.2.80] - 2025-09-07

- CI: set git identity, renumber steps, use repo-local CARGO_HOME in upstream-merge workflow. (6a5796a5)
- Meta: no functional changes; release metadata only. (56c7d028)

## [0.2.79] - 2025-09-07

- CI: harden upstream merge strategy to prefer local changes and reduce conflicts during sync for more stable releases. (b5266c7c)
- Build: smarter cleanup of reintroduced crates to avoid transient workspace breaks during upstream sync. (b5266c7c)

## [0.2.78] - 2025-09-07

- CI: harden upstream-merge flow, fix PR step order, install jq; expand cleanup to purge nested Cargo caches for more reliable releases. (07a30f06, aae9f7ce, a8c7535c)
- Repo: broaden .gitignore to exclude Cargo caches and local worktrees, preventing accidental files in commits. (59ecbbe9, c403db7e)

## [0.2.77] - 2025-09-07

- TUI/GitHub: add settings view for GitHub integration. (4f59548c)
- TUI/GitHub: add Actions tools to browse runs and jobs. (4f59548c)
- TUI: wire GitHub settings and Actions into bottom pane and chatwidget for quick access. (4f59548c)

## [0.2.76] - 2025-09-07

- CI: pass merge-policy.json to upstream-merge agent and use policy globs for safer merges. (ef4e5559)
- CI: remove upstream .github codex-cli images after agent merge to keep the repo clean. (7f96c499)

## [0.2.75] - 2025-09-07

- No user-facing changes; maintenance-only release with CI cleanup. (c5cd3b9e, 2e43b32c)
- Release: prepare 0.2.75 tag and metadata. (1b6da85a)

## [0.2.74] - 2025-09-06

- Maintenance: no user-facing changes; CI and repo hygiene improvements. (9ba6bb9d, 4ed87245)
- CI: guard self/bot comments; improve upstream-merge reconciliation and pass Cargo env for builds. (9ba6bb9d)

## [0.2.73] - 2025-09-06

- CI/Build: default CARGO_HOME and CARGO_TARGET_DIR to workspace; use sparse registry; precreate dirs for sandboxed runs. (dd9ff4b8)
- CI/Exec: enable network for workspace-write exec runs; keep git writes opt-in. (510c323b)
- CLI/Fix: remove invalid '-a never' in 'code exec'; verified locally. (87ae88cf)
- CI: pass flags after subcommand so Exec receives them; fix heredoc quoting and cache mapping; minor formatting cleanups. (854525c9, 06190bba, c4ce2088, 086be4a5)

## [0.2.72] - 2025-09-06

- Core/Sandbox: add workspace-write opt-in (default off); allow .git writes via CI override. (3df630f9)
- CI: improve upstream-merge push/auth and skip recursive workflows to stabilize releases. (274dcaef, 8fadbd03, dc1dcac0)

## [0.2.71] - 2025-09-06

- TUI/Onboarding: apply themed background to auth picker surface. (ac994e87)
- Login: remove /oauth2/token fallback; adopt upstream-visible request shape. (d43eb23e)
- Login/Success: fix background and theme variables. (c4e586cf)

## [0.2.70] - 2025-09-06

- TUI: add time-based greeting placeholder across composer, welcome, and history; map 10–13 to "today". (26b6d3c5, a97dc542)
- TUI/Windows: prevent double character echo by ignoring Release events without enhancement flags. (9e6b1945)
- Login: fallback to /oauth2/token and send Accept for reliable token exchange. (993c0453)
- TUI: fully reset UI after jump-back to avoid stalls when sending next message. (9d482af2)
- TUI/Chrome: allow specifying host for external Chrome connection (dev containers). (2b745f29)

## [0.2.69] - 2025-09-06

- TUI: add session resume picker (--resume) and quick resume (--continue). (234c0a04)
- TUI: show minutes/hours in thinking timer. (6cfc012e)
- Fix: skip release key events on Windows. (13a2ce78)
- Core: respect model family overrides from config. (ba9620ae)
- Breaking: stop loading project .env files. (db383473)

## [0.2.68] - 2025-09-06

- Core: normalize working directory to Git repo root for consistent path resolution. (520b1c3e)
- Approvals: warn when approval policy is missing to avoid silent failures. (520b1c3e)

## [0.2.67] - 2025-09-05

- TUI: prevent doubled characters on Windows by ignoring Repeat/Release for printable keys. (73a22bd6)
- CI: issue triage improves comment‑mode capture, writes DECISION.json, and adds token fallbacks for comment/assign/close steps. (8b4ea0f4, 544c8f15, 980aa10b)

## [0.2.66] - 2025-09-05

- No functional changes; maintenance-only release focused on CI. (a6158474)
- CI: triage workflow uses REST via fetch; GITHUB_TOKEN fallback. (731c3fce)
- CI: enforce strict JSON schema and robust response parsing. (22a3d846, b5eaecf4)
- CI: standardize Responses API usage and model endpoint selection. (118c4581, 9b8c2107, 73b73ba2)

## [0.2.65] - 2025-09-05

- Core: embed version via rustc-env; fix version reporting. (32c495f6)
- Release: harden publish flow; safer non-FF handling and retries. (6e35f47c)

## [0.2.63] - 2025-09-05

- TUI: inline images only; keep non-image paths as text; drop pending file tracking. (ff19a9d9)
- TUI: align composer/history wrapping; add sanitize markers. (9e3e0d86)
- Core: embed display version via tiny crate; remove CODE_VERSION env. (32f18333)

## [0.2.61] - 2025-09-05

- No functional changes; maintenance-only release focused on CI. (d7ac45c)
- CI: trigger releases only from tags; parse version from tag to prevent unintended runs. (15ad27a8)
- CI: reduce noise by enforcing [skip ci] on notes-only commits and ignoring notes-only paths. (52a08324, 12511ad2, c36ab3d8)

## [0.2.60] - 2025-09-05

- Release: collect all `code-*` artifacts recursively to ensure assets. (d9f9ebfd)
- Release notes: add Compare link and optional Thanks; enforce strict sections. (f7a5cc88, 84253961)
- Docs: use '@latest' in install snippet; tighten notes format. (b5aee550)

## [0.2.59] - 2025-09-05

- TUI: enforce strict global ordering and require stream IDs for stable per‑turn history. (7c71037d, 7577fe4b)
- TUI/Core: make cancel/exit immediate during streaming; kill child process on abort to avoid orphans. (74bfed68, 64491a1f)
- TUI: sanitize diff/output (expand tabs; strip OSC/DCS/C1/zero‑width) for safe rendering. (d497a1aa)
- TUI: add WebFetch tool cell with preview; preserve first line during streaming. (f6735992)
- TUI: restore typing on Git Bash/mintty by normalizing key event kind (Windows). (5b722e07)

## [0.2.56] - 2025-09-01

- Strict event ordering in TUI: keep exec/tool cells ahead of the final assistant cell; render tool results from embedded markdown; stabilize interrupt processing. (dfb703a)
- Reasoning titles: better collapsed-title extraction and formatting rules; remove brittle phrase checks. (5ca1670, 7f4c569, 6d029d5)
- Plan streaming: queue PlanUpdate history while streaming to prevent interleaving; flush on finalize. (770d72c)
- De-dup reasoning: ignore duplicate final Reasoning events and guard out-of-order deltas. (f1098ad)

## [0.2.55] - 2025-09-01

- Reasoning delta ordering: key by `(item_id, output_index, content_index)`, record `sequence_number`, and drop duplicates/out-of-order fragments. (b39ed09, 509fc87)
- Merge streamed + final reasoning so text is not lost on finalize. (2e5f4f8)
- Terminal color detection: unify truecolor checks; avoid 256-color fallback on Windows Terminal; smoother shimmer gradients. (90fdb6a)
- Startup rendering: skip full-screen background paint on Windows Terminal; gate macOS Terminal behavior behind `TERM_PROGRAM` and `CODE_FORCE_FULL_BG_PAINT`. (6d7bc98)

## [0.2.54] - 2025-09-01

- Clipboard image paste: show `[image: filename]` placeholders; accept raw base64 and data-URI images; enable PNG encoding; add paste shortcut fallback to read raw images. (d597f0e, 6f068d8, d4287d2, 7c32e8e)
- Exec event ordering: ensure `ExecCommandBegin` is handled before flushing queued interrupts to avoid out-of-order “End” lines. (74427d4)
- ANSI color mapping: fix 256-indexed → RGB conversion and luminance decisions. (ddf6b68)

## [0.2.53] - 2025-09-01

- Browser + HUD: add CDP console log capture, collapsible HUD, and coalesced redraws; raise expanded HUD minimum height to 25 rows. (34f68b0, 1fa906d, d6fd6e5, 95ba819)
- General: improve internal browser launch diagnostics and log path. (95ba819)

## [0.2.52] - 2025-08-30

- Diff rendering: sanitize diff content like input/output (expand tabs, strip control sequences) to avoid layout issues. (7985c70)

## [0.2.51] - 2025-08-30

- CLI: de-duplicate `validateBinary` to avoid ESM redeclare errors under Bun/Node 23. (703e080)

## [0.2.50] - 2025-08-30

- CLI bootstrap: make bootstrap helper async and correctly await in the entry; fixes Bun global installs when postinstall is blocked. (9b9e50c)

## [0.2.49] - 2025-08-30

- CLI install: bootstrap the native binary on first run when postinstall is blocked; prefer cached/platform pkg then fall back to GitHub release. (27a0b4e)
- Packaging: adjust Windows optional dependency metadata for parity with published packages. (030e9ae)

## [0.2.48] - 2025-08-30

- TUI Help: show environment summary and resolved tool paths in the Help panel. (01b4a8c)
- CLI install safety: stop publishing a `code` bin by default; create a wrapper only when no PATH collision exists and remove on collision to avoid overriding VS Code. (1a95e83)

## [0.2.47] - 2025-08-30

- Agents: add `/agents` command; smoother TUI animations and safe branch names. (0b49a37)
- Core git UX: avoid false branch-change detection by ignoring quoted text and tokenizing git subcommands; show suggested confirm argv when blocking branch change. (7111b30, a061dc8)
- Exec cells: clearer visual status — black ❯ on completed commands, tinting for completed lines, and concise tree guides. (f2d31bb)
- Syntax highlighting: derive syntect theme from the active UI theme for cohesive code styling. (b8c06b5)

## [0.2.46] - 2025-08-30

- CLI postinstall: print clear guidance when a PATH collision with VS Code’s `code` is detected; suggest using `coder`. (09ebae9)
- Maintenance: upstream sync prior to release. (d2234fb)

## [0.2.45] - 2025-08-30

- TUI “glitch” animation: compute render rect first, scale safely, and cap height; bail early on tiny areas. (8268dd1)
- Upstream integration: adopt MCP unbounded channels and Windows target updates while keeping forked TUI hooks. (70bd689, 3b062ea)
- CI/infra: various stability fixes (Windows cache priming; clippy profile; unbounded channel). (7eee69d, 5d2d300, 970e466, 3f81840)

## [0.2.44] - 2025-08-29

- Exec UX: show suggested confirm argv when branch-change is blocked. (a061dc8)
- File completion: prioritize CWD matches for more relevant suggestions. (7d4cf9b)
- Assistant code cards: unify streaming/final layout; refine padding and colors; apply consistent background for code blocks. (e12f31c, 986a764, e4601bd, 97a91e8, beaa1c7)
- Syntax highlighting: theme-aware syntect mapping for better readability. (b8c06b5)

## [0.2.43] - 2025-08-29

- npx/bin behavior: always run bundled binary and show exact path; stop delegating to system VS Code. (448b176)
- Postinstall safety: remove global `code` shim if any conflicting `code` is on PATH; keep `coder` as the entrypoint. (1dc19da)
- Exec cells: clearer completed-state visuals and line tinting. (f2d31bb)

## [0.2.42] - 2025-08-29

- Housekeeping: release and sync tasks for CLI, core, and TUI. (eea7d98, 6d80b3a)

## [0.2.41] - 2025-08-29

- Housekeeping: release and pre-sync commits ahead of broader upstream merges. (75bb264, 75ed347)

## [0.2.40] - 2025-08-29

- Upstream sync: align web_search events and TUI popup APIs; clean warnings; maintain forked behaviors. (f20bffe, 4d9874f)
- Features: custom `/prompts`; deadlock fix in message routing. (b8e8454, f7cb2f8)
- Docs: clarify merge-only push policy. (7c7b63e)

## [0.2.39] - 2025-08-29

- Upstream integration: reconcile core/TUI APIs; add pager overlay stubs; keep transcript app specifics; ensure clean build. (c90d140, b1b01d0)
- Tools: add “View Image” tool; improve cursor after suspend; fix doubled lines/hanging markers. (4e9ad23, 3e30980, 488a402)
- UX: welcome message polish, issue templates, slash command restrictions while running. (bbcfd63, c3a8b96, e5611aa)

## [0.2.38] - 2025-08-29

- TUI: code-block background styling and improved syntax highlighting. (bb29c30)
- Markdown: strip OSC 8 hyperlinks; refine rendering and syntax handling. (a30c019)
- Exec rendering: highlight executed commands as bash and show inline durations. (38dc45a)
- Maintenance: merged fixes from feature branches (`feat/codeblock-bg`, `fix/strip-osc8-in-markdown`). (6b30005, 0704775)

## [0.2.37] - 2025-08-27

- Packaging: move platform-specific binaries to npm optionalDependencies; postinstall resolves platform package before GitHub fallback. (5bb9d01)
- CI: fix env guard for NPM_TOKEN and YAML generation for platform package metadata. (7ae25a9, d29be0a)

## [0.2.36] - 2025-08-27

- Packaging: switch CI to produce a single `code` binary and generate `code-tui`/`code-exec` wrappers. (7cd2b18)
- CI: stabilize cargo fetch and Windows setup; adjust --frozen/--locked usage to keep builds reliable. (5c6bf9f, 5769cec, 7ebcd9f)

## [0.2.35] - 2025-08-27

- Release artifacts: slimmer assets with dual-format (.zst preferred, .tar.gz fallback) and stripped debuginfo; smaller npm package. (f5f2fd0)

## [0.2.34] - 2025-08-26

- Clipboard: add raw image paste support and upstream TUI integration; fix Windows path separators and ESC/Ctrl+C flow. (0c6f35c, 0996314, 568d6f8, e5283b6)
- UX polish: reduce bottom padding, improve rate-limit message, queue messages, fix italic styling for queued. (d085f73, ab9250e, 251c4c2, b107918)
- Stability: token refresh fix; avoid showing timeouts as “sandbox error”. (d63e44a, 17e5077)

## [0.2.33] - 2025-08-26

- Maintenance: housekeeping after successful build; release tag. (2c6bb4d)

## [0.2.32] - 2025-08-25

- Sessions: fast /resume picker with themed table and replay improvements. (0488753)
- Input UX: double‑Esc behavior and deterministic MCP tool ordering; fix build warnings. (b048248, ee2ccb5, fcf7435)
- Core/TUI: per-session ExecSessionManager; ToolsConfig fixes with `new_from_params`. (15af899, 7b20db9)

## [0.2.31] - 2025-08-25

- Diff wrapping: add one extra space to continuation hang indent for perfect alignment. (bee040a)

## [0.2.30] - 2025-08-24

- Diff summary: width-aware patch summary rendering with hanging indent; always show gutter icon at top of visible portion. (03beb32, 41b7273)

## [0.2.29] - 2025-08-24

- Version embedding: prefer `CODE_VERSION` env with fallback to Cargo pkg version across codex-rs; update banners and headers. (af3a8bc)

## [0.2.28] - 2025-08-24

- Windows toolchain: refactor vcpkg + lld-link config; ensure Rust binary embeds correct version in release. (9a57ec3, 8d61a2c)

## [0.2.27] - 2025-08-24

- Web search: integrate tool and TUI WebSearch event/handler; keep browser + agent tools; wire configs and tests. (6793a2a, a7c514a, 0994b78)
- CI: faster cross-platform linking/caching; streamlined Cargo version/lockfile updates. (c7c28f2, 5961330)

## [0.2.26] - 2025-08-24

- CI: improved caching and simplified release workflows for reliability. (e37a2f6, 8402d5a)

## [0.2.25] - 2025-08-24

- Release infra: multiple small workflow fixes (build version echo, Rust release process). (ac6b56c, 64dda2d)

## [0.2.24] - 2025-08-24

- Release workflow: update Rust build process for reliability. (64dda2d)

## [0.2.23] - 2025-08-24

- CI: fix build version echo in release workflow. (2f0bdd1)

## [0.2.22] - 2025-08-24

- Release workflow: incremental YAML fixes and cleanup. (3a88196, 7e4cea1)

## [0.2.21] - 2025-08-24

- CI cache: use `SCCACHE_GHA_VERSION` to restore sccache effectiveness. (43e4c05)

## [0.2.20] - 2025-08-24

- Docs: add module description to trigger CI and verify doc gating. (e4c4456)

## [0.2.19] - 2025-08-24

- CI: move sccache key configuration; tighten input responsiveness and diff readability in TUI. (46e57f0, 9bcf7c7)

## [0.2.18] - 2025-08-24

- TUI: clean unused `mut` and normalize overwrite sequences; preserve warning-free builds. (621f4f9)

## [0.2.17] - 2025-08-24

- TUI: housekeeping and stable sccache cache keys. (85089e1, 17bbc71)

## [0.2.16] - 2025-08-23

- Navigation: gate Up/Down history keys when history isn’t scrollable to avoid dual behavior. (150754a)

## [0.2.15] - 2025-08-23

- CI: stabilize sccache startup to fix slow releases. (f00ea33)

## [0.2.14] - 2025-08-23

- CI: small test to validate caching; no product changes. (7ebd744)

## [0.2.13] - 2025-08-23

- Build cleanliness: fix all warnings under build-fast. (0356a99)

## [0.2.12] - 2025-08-23

- CI: correct SCCACHE_DIR usage, export/guard env, and make caching resilient; better heredoc detection for apply_patch. (0a59600, b10c86a, c263b05, 39a3ec8, de54dbe)

## [0.2.11] - 2025-08-23

- Rendering: fully paint history region and margins to remove artifacts; add transcript hint and aggregated-output support. (b6ee050, ffd1120, eca97d8, 957d449)

## [0.2.10] - 2025-08-23

- Stability: align protocol/core with upstream; fix TUI E0423 and history clearing; regenerate Cargo.lock for locked builds. (52d29c5, 663d1ad, 2317707, da80a25)

## [0.2.9] - 2025-08-21

- Transcript mode: add transcript view; hide chain-of-thought by default; show “thinking” headers. (2ec5a28, e95cad1, 9193eb6)
- Exec ordering: insert running exec into history and replace in place on completion to prevent out-of-order rendering. (c1a50d7)
- Onboarding: split onboarding screen to its own app; improve login handling. (0d12380, c579ae4)

## [0.2.8] - 2025-08-21

- Exec previews: use middle-dot ellipsis and concise head/tail previews; rely on Block borders for visuals. (1ac3a67, 352ce75, 5ca0e06)

## [0.2.7] - 2025-08-20

- Browser tool: robust reconnect when cached Chrome WS URL is stale; clearer screenshot strategy and retries. (9516794)
- Merge hygiene and build fixes from upstream while keeping forked UX. (fb08c84, d79b51c)

## [0.2.6] - 2025-08-20

- History: live timers for custom/MCP tools; stdout preview for run commands; clearer background events. (f24446b, 5edbbe4, 2b9c1c9)
- Apply patch: auto-convert more shell-wrapped forms; suppress noisy screenshot-captured lines. (2fb30b7, 3da06e5)

## [0.2.5] - 2025-08-19

- CLI downloads: verify Content-Length, add timeouts/retries, and improve WSL guidance for missing/invalid binaries. (ca55c2e)

## [0.2.4] - 2025-08-19

- Windows CLI: guard against corrupt/empty downloads; clearer spawn error guidance (EFTYPE/ENOEXEC/EACCES). (bb21419)

## [0.2.3] - 2025-08-19

- Release CI: enable sccache and mold; tune incremental to improve cache hit rate. (69f6c3c)

## [0.2.2] - 2025-08-19

- Protocol alignment and dep bumps across codex-rs; login flow async-ified; smaller fixes. (4db0749, 6e8c055, 38b84ff)

## [0.2.1] - 2025-08-19

- Fork stabilization: large upstream sync while preserving TUI/theme and protocol; add tests and clean colors/styles. (b8548a0, 47ba653, c004ae5)

## [0.1.13] - 2025-08-16

- Rebrand: switch npm bin to `code`, handle collisions; rename Coder → Code across UI and docs. (0f1974a, b3176fe)
- TUI polish: glitch animations, status handling, stabilized scroll viewport; improved token footer and search suffix. (3375965, 2e42af0, 96913aa, 80fe37d)
- Core: Rust login server port; sandbox fixes; exec timer; browser console tool. (e9b597c, c26d42a, 2359878, d6da1a4)

## [0.1.12] - 2025-08-14

- CI/build: switch to rust-cache; fix sccache error; optimize builds; improve terminal query and image init. (2d1d974, eb922a7, 3055068, 9ca7661)

## [0.1.11] - 2025-08-14

- Release hygiene: fix version embedding and PowerShell replacement on Windows. (537f50b, 5d50fff)

## [0.1.10] - 2025-08-14

- MCP/Reasoning: JSON‑RPC support; enable reasoning for codex‑prefixed models; parse reasoning text. (e7bad65, de2c6a2, f1be797)
- TUI: diff preview color tweak, standardized tree glyphs, ctrl‑b/ctrl‑f shortcuts. (d4533a0, bb9ce3c, 0159bc7)
- CI/docs: restore markdown streaming; interrupt/Esc improvements; user‑agent; tracing; rate‑limit delays respected. (6340acd, 12cf0dd, cb78f23, e8670ad, 41eb59a)

## [0.1.9] - 2025-08-13

- Debug logging system and better conversation history; remove unused APIs. (92793b3, 34f7a50)

## [0.1.8] - 2025-08-13

- TUI history: correct wrapping and height calc; prevent duplication; improve JS harness for browser. (dc31517, 98b26df, 58fd385, 7099f78)

## [0.1.7] - 2025-08-12

- Rebrand foundation: fork as just‑every/coder; major TUI styling/animation upgrades. (aefd1e5, e2930ce)
- Browser: CDP connect to local Chrome with auto‑discovery, port parsing and stability fixes. (006e4eb, 1d02262, b8f6bcb, 756e4ea)
- Agents HUD: live agent panel with status; animated sparkline; improved focus behavior. (271ded3, e230be5, 0b631c7)

## [0.1.6] - 2025-08-12

- TUI: show apply‑patch diff; split multiline commands; ctrl‑Z suspend fix. (9cd5ac5, 55f9505, 320f150)
- Prompts: prompt cache key and caching integration tests. (7781e4f, 0a6cba8)
- CI/build: resolve workflow compilation errors; dependency bumps; docs refresh. (7440ed1, 38a422c, d17c58b)

## [0.1.5] - 2025-08-12

- Theme UI: live preview and wrapping fixes; improved input (double‑Esc clear, precise history). (96a5922, 1f68fb0)
- Layout: browser preview URL tracking and layout reorg; mute unnecessary mut warnings. (47bc272, 3778243)

## [0.1.4] - 2025-08-12

- Fork enhancements: mouse scrolling, glitch animation, status bar, improved TUI; configurable agents and browser tools with screenshots. (5d40d09, 55f72d7, a3939a0, cab23924)
- Packaging: shrink npm package by downloading binaries on install; fix Windows builds and permissions. (aea9845, 240efb8, 2953a7f)
- Workflows: align release pipeline; fix conflicts/warnings post‑merge. (f2925e9, 52bd7f6, ae47b2f)

## [0.1.3] - 2025-08-10

- Release pipeline cleanup: handle existing tags/npm version conflicts; drop redundant workflow. (cc243b1, 1cc2867)

## [0.1.2] - 2025-08-10

- Initial fork releases: set up rebrand + npm publishing; simplified release workflow; cross‑compilation fixes. (ff8378b, 3676c6a, 40d17e4, 1914e7b)
