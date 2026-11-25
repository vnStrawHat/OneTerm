# Low-Level Design: Non-dock component API drift

Intake: IN-0017
HLD: ../high-level-design.md
Topic: component-api-drift
Date: 2026-09-07

## Concern

Every v0.6.0 breaking change outside the dock that OneTerm actually touches, plus an explicit
list of the breaking changes that do *not* apply — so P3 is a bounded checklist rather than a
compile-error hunt.

## Design

### 1. `setting::RenderOptions` fields became private (10 sites, 2 files)

Upstream made the fields private with builders and accessors so new options can be added
compatibly. OneTerm reads them positionally to build keyed-state keys and to branch on layout.

```diff
- options.page_ix, options.group_ix, options.item_ix
+ options.page_ix(), options.group_ix(), options.item_ix()

- if options.layout.is_horizontal()
+ if options.layout().is_horizontal()

- .disabled(options.disabled)
- .with_size(options.size)
+ .disabled(options.is_disabled())
+ .with_size(options.size())
```

Affected: `crates/settings-ui/src/terminal/font.rs` (lines 139, 203, 279, 342, 343, 345),
`crates/settings-ui/src/terminal/logging.rs` (105, 126, 134, 145). Purely mechanical; the
compiler finds all of them.

Also renamed in the same upstream change, but **not used by OneTerm**:
`InputContextMenuCapabilities`, `InputPresentation`, `CalendarItemState`,
`ComboboxTriggerCtx` → `ComboboxTriggerContext`.

### 2. `InputState` is single-line only → `TextareaState` (1 site)

`InputState::multi_line(true)` is gone; multiline moves to a separate type.
`crates/app/src/crash_report_dialog.rs` is the only site:

```diff
- use gpui_component::input::{Input, InputState};
+ use gpui_component::input::{Textarea, TextareaState};

- InputState::new(window, cx).multi_line(true).default_value(report)
+ TextareaState::new(window, cx).default_value(report)
- Input::new(&state)
+ Textarea::new(&state)
```

Note the crash-report dialog is read-only display of a report; check whether it also relied on
input adornments (`prefix`, `suffix`, clear button), which are **not** properties of `Textarea` —
they must be composed around the control instead. The other 20-odd `InputState` sites
(auth form, connect/session/quick-connect dialogs, session search, number inputs) are all
single-line and unaffected.

### 3. Assets crate renamed (1 site + 2 manifests)

`gpui-component-assets` → `gpui-kit-assets`, Rust path `gpui_kit_assets`. Only
`crates/app/src/assets.rs` imports it (`use gpui_component_assets::Assets`), where it is the
fallback source behind OneTerm's own `UiAssets`. Behavior is unchanged; the merge order in
`CustomAssets::load` / `list` stays as-is.

### 4. Settings-scroll behavior: vendor patch 0003 is half-fixed upstream

This is the one item that is not a rename, and it decides whether the vendor tree can go.
OneTerm's patch 0003 makes two changes:

| Patch hunk | v0.6.0 upstream state |
| --- | --- |
| `setting/page.rs`: `ListState::new(..).measure_all()` so `scroll_to_reveal_item` has real heights | **Superseded.** Upstream replaced the eager-measure approach with `SettingsState::deferred_scroll_group_ix` — the sidebar records the target group, and `SettingPage::render` consumes it and calls `scroll_to_reveal_item(ix)` during the render that follows. Drop this hunk. |
| `setting/settings.rs`: `.enumerate()` **before** `.filter(\|g\| g.title.is_some())` so untitled groups do not offset the index | **Still present upstream.** v0.6.0 `render_sidebar` still does `.iter().filter(\|g\| g.title.is_some()).enumerate()`, so `group_ix` counts only titled groups while `deferred_scroll_group_ix` indexes the full group list. |

So the bug OneTerm patched still exists in 0.6.0, and it is reachable: `crates/settings-ui/src/about.rs`
has an untitled group at index 0 (the app-identity block) followed by a titled `"Links"` group at
index 1, so clicking `"Links"` in the sidebar scrolls to group 0. The key-bindings page builds
groups dynamically and may hit the same thing.

Since the vendor tree is being retired, fix it from OneTerm's side. Options, in preference order:

1. **Give every `SettingGroup` a title.** The filter then drops nothing and the indices align, with
   no upstream change. Costs a visible heading on the About page's identity block — a design
   change, so it needs a look before being adopted.
2. **Order untitled groups last on every page.** Indices align for all titled groups without any
   visual change. Fragile: a future page that puts an untitled group first reintroduces the bug
   silently, so it needs a comment and ideally a test.
3. **Send the one-line fix upstream** and carry option 2 until it lands. Preferred long-term;
   does not block this migration.

Whichever is chosen, add a focused test asserting that clicking each sidebar group scrolls to the
matching group, so the behavior is pinned from OneTerm's side rather than depending on upstream.

### 5. Breaking changes that do NOT apply — verified against the tree

Recording these so P3 does not re-investigate them:

| Upstream breaking change | Why it does not apply |
| --- | --- |
| `Table` → `DataTable`, `row_selector` → `row_header` | SFTP already uses `DataTable` + `TableState` + `TableDelegate`; no `row_selector` anywhere |
| `ListDelegate::is_eof` / `TableDelegate::is_eof` → `has_more` | Neither is implemented; `SftpTableDelegate` has no pagination, `SearchableListDelegate` does not use it |
| `divider::Divider` → `separator::Separator`, `DescriptionList::divider()` | No `Divider` use; OneTerm's own `separators.rs` builds a `div()` |
| `webview` feature removed → `gpui-wry` | No webview use |
| `History<T: HistoryItem>` removed, split into `History` / `UndoHistory` | Not used |
| `Editor` / `EditorState`, markdown, charts, code folding | Not used |
| `Dialog::new(window, cx)` → `Dialog::new(cx)` | OneTerm builds dialogs through its own `oneterm_state::form_dialog::FormDialog` over `DialogContent` / `DialogFooter` / `DialogButtonProps`; no direct `Dialog::new`. Verify `FormDialog` still compiles — it is the one shared wrapper. |
| Chart `Tooltip::new(cursor, bounds)`, `ScaleBand` bounds | No charts |
| `popover_style` moved from `StyledExt` to `ThemeStyled` | Not called |
| `can_go_to_definition()` → `has_definition()` | Editor-only |
| `animation::Transition` deprecated → `EffectTransition` | Not called |

### 6. Things to re-check because they are re-exports, not renames

- `gpui_component::resizable` (16 sites) — kept as a backwards-compatible module re-exporting
  `ResizableState`, `h_resizable`, `v_resizable`, `resizable_panel` from `gpui_base`. Should
  compile untouched; confirm rather than assume.
- `Root`, `TitleBar`, `WindowExt`, `GlobalState`, `Theme`, `ThemeMode`, `ThemeRegistry`,
  `ActiveTheme`, `Icon`, `IconName`, `IconNamed`, `icon_named!`, `h_flex`, `v_flex`, `Sizable`,
  `scroll::ScrollbarShow` — all still exported from `gpui_component`'s root or the same
  submodules. `alert`, `notification`, `menu`, `tree`, `tooltip`, `checkbox`, `button`, `dialog`,
  `input`, `table`, `setting`, `searchable_list` modules all still exist.
- `gpui_component::init(cx)` still exists and still initializes `gpui-base` too, so
  `crates/app/src/lib.rs` and `crates/app/src/init.rs` keep their current shape.
- `cx.global_mut::<gpui_component::Theme>().mono_font_family = "Lilex".into()` — the theme now
  also carries gradient backgrounds, configurable focus rings and new semantic colors. OneTerm's
  themes in `crates/theme/themes/*.json` are validated against `.theme-schema.json`; new optional
  fields should not break them, but the schema reference copy must be refreshed and the built-in
  themes re-checked for dark-mode syntax-highlighting changes.

## Interfaces

```rust
// settings-ui
options.page_ix() / group_ix() / item_ix() / size() / layout() / is_disabled() / group_variant()

// app/crash_report_dialog.rs
gpui_component::input::{Textarea, TextareaState}

// app/assets.rs
gpui_kit_assets::Assets
```

## Edge Cases and Failure Modes

- [ ] The crash-report `Textarea` loses read-only behavior or sizing that `multi_line` implied.
- [ ] The settings-scroll fix is dropped with the vendor tree and nobody notices, because no test
      covers sidebar-group navigation today.
- [ ] `FormDialog` breaks on a `DialogContent` / `DialogFooter` / `DialogButtonProps` signature
      change; it is shared by five dialogs, so one break hits SSH connect, quick connect, session
      edit, group rename and SFTP actions at once.
- [ ] A theme JSON silently loses a color because the schema gained a required field.
- [ ] `rust-i18n` locale handling changed (`gpui_component::locale` / `set_locale`); OneTerm does
      not call either, but the crate is now initialized with `fallback = "en"`, so a missing key
      renders English rather than the key name — cosmetic only.

## Verification

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] Settings window: every sidebar group click scrolls to the correct section, including on the
      About page (the untitled-group case) — new focused test plus manual check.
- [ ] Crash-report dialog renders the full report, scrolls, and copies.
- [ ] All five `FormDialog` callers open, validate, and submit.
- [ ] Every built-in theme in `crates/theme/themes/` loads and renders in both light and dark.
- [ ] Custom `AppIcon` SVGs still resolve (`UiAssets` before `gpui_kit_assets::Assets`).
