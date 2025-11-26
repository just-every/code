# Auto Drive State Inventory

This document catalogs every `auto_state` field access across the TUI and controller so we can migrate toward single-phase semantics without missing any flag interactions.

## Sources Scanned

- `code-rs/tui/src/chatwidget.rs`
- `code-rs/tui/src/bottom_pane/auto_coordinator_view.rs`
- `code-rs/tui/src/bottom_pane/auto_drive_settings_view.rs`
- `code-rs/tui/src/bottom_pane/paste_burst.rs`
- `code-rs/tui/src/chatwidget/smoke_helpers.rs`
- `code-rs/code-auto-drive-core/src/controller.rs`

Each entry below lists read vs. write occurrences (line numbers and snippets). Counts help highlight high-traffic fields.

## Field Classification

| Field | Category | Notes |
| --- | --- | --- |
| `active` | Phase control | Primary on/off latch for Auto Drive; should collapse into `AutoRunPhase::Active`/`Idle`. |
| `awaiting_submission` | Phase control | Drives countdown and gating for prompt submission; redundant with `AutoRunPhase::AwaitingCoordinator`. |
| `waiting_for_response` | Phase control | Distinguishes coordinator wait vs. live-streaming response; overlaps with `AwaitingDiagnostics`. |
| `paused_for_manual_edit` | Phase control | Legacy manual-edit gate; duplicative of `AutoRunPhase::PausedManual`. |
| `resume_after_manual_submit` | Phase control | Remembers whether manual submits should resume automatically; belongs inside `PausedManual` payload. |
| `waiting_for_review` | Phase control | Tracks post-turn review gating; mirrors `AutoRunPhase::AwaitingReview`. |
| `waiting_for_transient_recovery` | Phase control | Marks exponential backoff windows; mirrors `AutoRunPhase::TransientRecovery`. |
| `coordinator_waiting` | UI view data | Indicates coordinator prompt handshake state; used for progress copy and hint toggles.

## `active` (phase control)

- Reads (8)
  - `code-rs/tui/src/chatwidget.rs:14153` — guard before mutating review UI while idle.
  - `…:14307` — skip coordinator pane when idle.
  - `…:14587` — hide goal banner unless active or awaiting goal.
  - `…:14613` — bail on streaming renderer unless active.
  - `…:14646` — hide progress plaque while idle.
  - `…:14675` — stop summary aggregation if run stopped.
  - `…:14762` — guard reasoning title updates.
  - `…:23380` — keep review shortcuts disabled when idle.
- Writes (10)
  - `code-rs/tui/src/chatwidget.rs:21879` — smoke helper seeds active state for tests.
  - Additional nine test helpers (`…:22020`, `…:22047`, `…:22128`, `…:22194`, `…:22250`, `…:22304`, `…:22333`, `…:22394`, `chatwidget/smoke_helpers.rs:311`).
  - Controller mirror updates appear in `sync_booleans_from_phase()` (`code-auto-drive-core/src/controller.rs:263`, `273`, `283`, `293`, `303`, `313`, `323`, `479`, `510`).

## `awaiting_submission` (phase control)

- Reads (5)
  - `code-rs/tui/src/chatwidget.rs:14570` — decides whether to elide ellipsis on summary lines during pending submit.
  - Controller helper logic uses it (`code-auto-drive-core/src/controller.rs:678`, `699`, `774`, `841`).
- Writes (9)
  - All originate from controller transitions (`controller.rs:264`, `274`, `284`, `294`, `304`, `314`, `324`, `568`, `655`).
  - No TUI caller writes directly.

## `waiting_for_response` (phase control)

- Reads (10)
  - Reactive UI checks (`chatwidget.rs:13215`, `14153`, `14403`, `14434`, `14514`, `14528`, `14773`).
  - Test assertions (`chatwidget.rs:22037`, `22115`, `22184`).
- Writes (10)
  - TUI clears after finalization (`chatwidget.rs:13557`).
  - Smoke helpers and fixtures seed state (`chatwidget.rs:22021`, `22049`, `22130`, `22396`).
  - Controller toggles across transitions (`controller.rs:265`, `275`, `285`, `295`, `305`, `315`, `325`, `336`, `498`, `566`).

## `paused_for_manual_edit` (phase control)

- Reads (6)
  - Manual editor banner (`chatwidget.rs:14369`).
  - Controller helper guards (`controller.rs:678`, `700`, `775`, `830`, `841`).
- Writes (8)
  - Solely controller-managed (`controller.rs:267`, `277`, `287`, `297`, `307`, `317`, `327`, `569`).

## `resume_after_manual_submit` (phase control)

- Reads (1)
  - Manual resume decision (`controller.rs:836`).
- Writes (9)
  - Controller clears or copies flag during transitions (`controller.rs:268`, `278`, `288`, `298`, `308`, `318`, `328`, `356`, `570`).

## `waiting_for_review` (phase control)

- Reads (13)
  - Review UI gating and tests (`chatwidget.rs:13819`, `14182`, `14378`, `22032`, `22093`, `22099`, `22174`, `22180`, `22221`, `22437`, `22452`, `23380`).
  - Phase helper fallback (`controller.rs:845`).
- Writes (9)
  - Controller transitions (`controller.rs:269`, `279`, `289`, `299`, `309`, `319`, `329`, `571`).
  - ChatWidget clears on forced stop (`chatwidget.rs:23386`).

## `waiting_for_transient_recovery` (phase control)

- Reads (1)
  - Phase helper fallback (`controller.rs:850`).
- Writes (9)
  - ChatWidget clears before scheduling restart (`chatwidget.rs:13456`).
  - Controller transitions (`controller.rs:270`, `280`, `290`, `300`, `310`, `320`, `330`, `383`, `565`).

## `coordinator_waiting` (UI view data)

- Reads (3)
  - Coordinator progress hints (`chatwidget.rs:14403`, `14434`, `14529`).
- Writes (12)
  - ChatWidget resets on completion (`chatwidget.rs:13400`, `13558`).
  - Controller mirrors during transitions (`controller.rs:266`, `276`, `286`, `296`, `306`, `316`, `326`, `340`, `499`, `567`).

## Current Boolean Combinations

- `waiting_for_response && !coordinator_waiting` — drives “model is thinking” messaging and hides coordinator countdown. (`chatwidget.rs:14403`)
- `awaiting_submission && !paused_for_manual_edit` — determines when countdown and auto-submit button should be live. (`controller.rs:678`, `chatwidget.rs:14501`)
- `active && waiting_for_review` — blocks manual resume until review flow resolves. (`chatwidget.rs:22093`)
- `is_paused_manual() && should_bypass_coordinator_next_submit()` — keeps manual edit overlay while skipping coordinator prompt; helper now derives directly from `AutoRunPhase::PausedManual { bypass_next_submit }`. (`chatwidget.rs:14045` vicinity)
- `in_transient_recovery() && waiting_for_transient_recovery` — redundant gating around restart timer, still split between phase helper and boolean. (`controller.rs:850`)

These combinations highlight where the new `AutoRunPhase` variants must carry the data currently modeled via multiple booleans, allowing legacy mirrors to be dropped once consumers migrate.
