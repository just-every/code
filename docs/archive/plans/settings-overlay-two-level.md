# Two-Level Settings Overlay Design

Last updated: 2025-10-16

## Goals

- Introduce an explicit two-phase UX for the Settings overlay: a top-level
  menu (“Menu mode”) and focused section views (“Section mode”).
- Reserve navigation keys so `↑`/`↓` move within the menu, `Enter`/`→` drill
  into a section, `Esc`/`←` navigate back toward the menu, and `Esc` from the
  menu closes the overlay.
- Add a breadcrumb/header row that clarifies where the user is and surfaces
  the primary key hints without overloading the existing section content.
- Preserve the existing section renderers and business logic with minimal
  rewrites (Agents/Limits integrations remain as-is inside Section mode).

## Non-Goals

- Rewriting individual section UIs (e.g., the Agents or Limits editors).
- Changing how slash commands and AppEvents populate section data (they just
  target the correct mode).
- Adding new settings sections—this refactor only restructures navigation.

## Current State Summary

- `SettingsOverlayView` renders a sidebar and section content simultaneously,
  with `↑/↓`, `Tab`, and `←/→` all selecting sections. `Esc` always closes the
  overlay.
- Section views also consume `←/→` for in-form toggles (Agents, MCP, etc.) and
  occasionally use `Tab` for intra-form navigation.
- There is no explicit state to distinguish “menu focus” vs. “section focus”; a
  slash command jumps directly into a section but the key handlers still assume
  global focus.
- Header chrome is a simple hint row (no breadcrumb, limited context).

The combination leads to confusing focus: pressing `↑/↓` inside a section may
  unexpectedly switch sections instead of scrolling the current content, and
  `Esc` provides no intermediate “back to menu” step.

## Proposed Architecture

### High-Level State Machine

Introduce an overlay mode enum that lives inside `SettingsOverlayView`:

```rust
enum SettingsOverlayMode {
    Menu(MenuState),
    Section(SectionState),
}
```

- `MenuState` tracks the highlighted section index and optional metadata (e.g.,
  last-opened timestamp for future polish).
- `SectionState` stores the active `SettingsSection` and whether the user got
  there from a menu selection vs. a direct AppEvent (used for breadcrumb and
  “go back” logic).

`SettingsState` in `chatwidget.rs` continues to hold `Option<SettingsOverlayView>`;
  the view now encapsulates the mode, menu selection, and section content.

### Data Model Additions

- `SettingsOverlayView` gains:
  - `mode: SettingsOverlayMode`
  - `last_section: SettingsSection` (used to pre-select a menu row when
    reopening).
  - helper accessors: `is_menu_active()`, `active_section()`,
    `set_mode_menu(selected: Option<SettingsSection>)`, and
    `set_mode_section(section: SettingsSection)`.
- `MenuState` holds a vector of `MenuItem` structs with label, optional icon,
  description, and section pointer.
- `BreadcrumbHeader` helper (see Rendering) stores the current breadcrumb
  segments and key hint strings.

### Interaction Flow

1. Opening `/settings` calls `show_settings_overlay_full(None)`:
   - Overlay instantiates in `SettingsOverlayMode::Menu`, highlighting the last
     visited section (default `SettingsSection::Model`).
2. User presses `Enter`/`→` to drill into the highlighted section. Overlay
   switches to `Section` mode, breadcrumb updates to `Settings ▸ {Section}`.
3. Inside `Section` mode:
   - `←` or `Esc` returns to the menu (same selection retained).
   - Content-specific keys (`Tab`, letter shortcuts, etc.) remain scoped.
4. From `Menu` mode, `Esc` closes the overlay (and triggers `notify_close`).
5. Slash commands or AppEvents that need a specific section call
   `ensure_settings_overlay_section`, which now:
   - Creates the overlay if missing.
   - Sets mode to `Section` and ensures the menu highlight follows the section
     so the user’s back navigation is predictable.

### Key Handling Changes

- Update `handle_settings_key` (and any direct overlay key entry points) to
  branch on `mode` before delegating.
  - **Menu mode**: accept `↑/↓`, `k/j` (existing aliases), alphanumeric
    shortcuts, `Home`, `End`; `Enter/Right` transitions to Section; `Esc`
    closes; other keys ignored.
  - **Section mode**: forward keys to the active `SettingsContent` first. If the
    content does not consume the key, handle `Esc`/`Left` by switching back to
    Menu. Suppress `Tab`/`Shift+Tab` switching sections—the content gets those.
- As part of this refactor we will remap conflicting section keys per the
  keymap audit:
  - Replace `←/→` toggles in section views with `Space`/`Enter` or explicit
    letter shortcuts (`agents_settings_view`, `mcp_settings_view`, etc.).
  - Limit top-level section cycling to `↑/↓` (drop `Tab`/`Shift+Tab`).
  - Retain `PgUp/PgDn` semantics inside content views; they are unaffected.

### Rendering Updates

- **Breadcrumb header**: introduce a lightweight component (see agent proposal)
  that renders two rows:
  1. Breadcrumb path: `Settings ▸ Agents` (menu displays `Settings ▸ Menu`).
  2. Key hints: `↑/↓ move  Enter open  Esc back  ? help` (Menu) or
     `←/Esc back  Tab cycle  ? help` (Section).
- **Menu layout**:
  - When in `Menu` mode, the body area renders a vertical list of sections with
    label, short description (≤ 60 chars), and an ASCII “icon” token. Suggested
    tokens:

    | Section         | Token | Short description (≤60 chars)       |
    |-----------------|-------|-------------------------------------|
    | Agents          | `[AG]`| Manage assistants and defaults      |
    | Model           | `[ML]`| Pick provider, temperature, safety  |
    | Limits          | `[LM]`| View and tune rate-limit guards     |
    | Chrome          | `[CH]`| Connect the Chrome bridge           |
    | GitHub          | `[GH]`| Link repos and choose scopes        |
    | Updates         | `[UP]`| Check version & release channel     |
    | Validation      | `[VL]`| Configure pre-flight validations    |
    | Notifications   | `[NT]`| Choose desktop / in-app alerts      |
    | Theme           | `[TH]`| Switch palette, fonts, density      |
    | MCP             | `[MC]`| Manage MCP servers & permissions    |

    (UI can optionally swap in emoji later; ASCII placeholders keep rendering
    deterministic.)
- **Section layout**:
  - When in `Section` mode, reuse the existing content renderer. The menu list
    is hidden (or shrunk to 0 width) so keys and focus stay local to the
    content area.
  - Breadcrumb header ensures users can hop back.

### AppEvent and Command Wiring

- Update `ChatWidget::ensure_settings_overlay_section`/
  `show_settings_overlay_full` so any external entry point:
  1. Ensures the overlay exists.
  2. Sets `mode = Section(section)` and syncs the menu highlight.
  3. Prefills Section content and requests redraw.
- Adjust AppEvent handlers (`ShowAgentsOverview`, `ShowAgentEditor`, etc.) to
  call the new helpers, but **do not** bypass the menu; returning via `Esc`
  should land on the proper menu selection.
- Slash command handlers for `/settings`, `/agents`, `/limits`, `/model`, etc.
  call `show_settings_overlay_full` with either `None` (menu) or `Some(section)`.

### Persistence Considerations

- When the overlay closes from Section mode, we automatically revert to Menu
  mode and reselect the active section before dismissal so reopening starts at
  the same menu row.
- Optionally persist `last_section` in `SettingsState` if the overlay is torn
  down and recreated frequently.

## Testing & Verification Plan

- **Snapshot coverage**: add VT100 snapshots for
  - Menu-only rendering (fresh open, highlight first section).
  - Drill-down flow (open Agents, ensure breadcrumb updates, verify ESC twice
    path).
  - Keyboard guard rails (simulate `Esc` in Section mode ensures menu visible).
- **Unit tests**:
  - Mode transition helpers (`set_mode_menu`, `set_mode_section`).
  - `handle_settings_key` behavior table-driven by mode.
- **Manual checks**:
  - Ensure each section’s custom key handling still works (esp. Agents editor,
    Limits tabs) and no longer steals `←/→` reserved for overlay back/forward.

## Incremental Rollout Strategy

1. **Scaffold modes**: Introduce `SettingsOverlayMode`, restructure rendering to
   hide content while in Menu mode, but leave key bindings temporarily shared.
2. **Keymap cleanup**: Apply the remaps highlighted in the key audit so section
   content no longer requires `←/→` for core actions.
3. **Mode-aware key handling**: Update `handle_settings_key` and friends to
   branch on mode, wire breadcrumb header, and adjust ESC logic.
4. **Slash command / AppEvent integration**: Ensure all entry points set the
   mode correctly and request redraws.
5. **Docs/help refresh**: Update slash command docs and inline help once the UX
   is firm.
6. **Testing sweep**: Add VT100 snapshots, run `./build-fast.sh`, and manually
   validate navigation.

Each step keeps the overlay functional, allowing incremental PRs if desired.

## Open Questions / Follow-Ups

- Should we expose accelerator keys in the menu (e.g., `M` jumps to Model)? If
  yes, we can display shortcut glyphs alongside the menu rows (the current
  overlay already supports this).
- Do we want to remember “last drill-down section” between app launches? That
  would require persisting the selection in user settings.
- The ASCII icon tokens may eventually need a design pass—are emoji acceptable
  in the TUI for higher visual separation?
- Some section views (Agents editor) still surface their own “Esc closes” copy;
  we should revisit wording once the new breadcrumb/back flow lands.

