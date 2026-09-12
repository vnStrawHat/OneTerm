# IN-0029 research — behavioural specification of the vendored VT engine

Extracted from the vendored sources that a from-scratch engine must replace:

| Tree | Version / provenance | LOC |
|---|---|---|
| `vendor/vte` | crates.io `vte 0.15.0` (VCS `3b3da71c…`) + OneTerm patches 0001, 0002 | ~4.1k |
| `vendor/alacritty_terminal` | `github.com/zed-industries/alacritty` @ `fcf32feacb367b75ec84dd40f041e4fd411d3cc1` + OneTerm patches 0001–0003 | ~12.5k |

Both are `Apache-2.0` (`vendor/vte/Cargo.toml:44`, `alacritty_terminal/Cargo.toml:5`). All line
citations are `path:line` against the vendored trees unless a different root is named.

Files deliberately **not** specified here (noted only for completeness):

- `alacritty_terminal/src/vi_mode.rs` (893 L) — keyboard-driven vi cursor motions. It touches `Term`
  only through `vi_mode_cursor` and `selection`; the engine must keep the *field* and the clamping
  rules listed in §2.15 but the motion table itself is out of scope.
- `alacritty_terminal/src/term/search.rs` (1251 L) — regex search (`RegexSearch`, `RegexIter`) plus
  the semantic/line/bracket search helpers that `selection.rs` depends on (§2.19).
- `alacritty_terminal/src/event_loop.rs` (486 L) — the PTY reader/writer thread. Not part of the VT
  engine, but two constants leak into engine behaviour: `READ_BUFFER_SIZE = 0x10_0000`
  (`event_loop.rs:24`) and `MAX_LOCKED_READ = u16::MAX` (`event_loop.rs:27`); see §5.
- `alacritty_terminal/src/tty/unix.rs` (706 L) — `forkpty`/`openpty`, `SIGCHLD`, `SignalMask`. The
  Windows equivalent is specified in §6.

---

## 1. `vte` — the byte-level parser

### 1.1 Shape

`Parser` (`lib.rs:55-70`) is a hand-written Paul-Williams state machine with these fields:

| Field | Purpose | Limit |
|---|---|---|
| `state: State` | current state, default `Ground` (`lib.rs:729-744`) | 14 states |
| `intermediates: [u8; 2]`, `intermediate_idx` | collected `0x20..=0x2F` (and CSI/DCS private markers) | `MAX_INTERMEDIATES = 2` (`lib.rs:43`) |
| `params: Params`, `param: u16` | CSI/DCS numeric params + subparams | `MAX_PARAMS = 32` (`params.rs:5`) |
| `osc_raw: Vec<u8>` (std) / `ArrayVec` (no_std) | OSC payload bytes | unbounded with `std`; `MAX_OSC_RAW = 1024` only in `no_std` (`lib.rs:45`, `lib.rs:60-63`) |
| `osc_params: [(usize,usize); 16]`, `osc_num_params` | `;`-separated slices into `osc_raw` | `MAX_OSC_PARAMS = 16` (`lib.rs:44`) |
| `ignoring: bool` | overflow flag passed to dispatch as `ignore` | |
| `partial_utf8: [u8;4]`, `partial_utf8_len` | UTF-8 continuation across `advance()` calls | |

Two entry points (`lib.rs:109`, `lib.rs:140`):

- `advance(&mut self, perf, bytes)` — consume all of `bytes`.
- `advance_until_terminated(perf, bytes) -> usize` — same, but stops as soon as
  `Perform::terminated()` returns `true` after a dispatch; returns bytes consumed. This is the
  hook the synchronized-update machinery uses (§1.9).

### 1.2 States and transitions

`change_state` (`lib.rs:168-185`) dispatches per state; `Ground` is handled separately by
`advance_ground` (`lib.rs:590-660`) which uses `memchr` for the next `ESC`.

| State | Byte class | Action |
|---|---|---|
| **Ground** (`lib.rs:590`) | scan to next `0x1B` | valid UTF-8 run → `print(c)` per char, except `\x00..=\x1f` and `\u{80}..=\u{9f}` → `execute(c as u8)` (`ground_dispatch`, `lib.rs:713-721`) |
| | `0x1B` | `reset_params()`, → `Escape` |
| **Escape** (`lib.rs:340`) | `00-17,19,1C-1F` | `execute` |
| | `20-2F` | `collect`, → `EscapeIntermediate` |
| | `30-4F`, `51-57`, `59-5A`, `5C`, `60-7E` | `esc_dispatch(intermediates, ignore, byte)`, → `Ground` |
| | `50` (`P`) | `reset_params()`, → `DcsEntry` |
| | `58` (`X`), `5E` (`^`), `5F` (`_`) | → `SosPmApcString` |
| | `5B` (`[`) | `reset_params()`, → `CsiEntry` |
| | `5D` (`]`) | clear `osc_raw`, `osc_num_params = 0`, → `OscString` |
| | `18`/`1A` | `execute`, → `Ground` |
| | `1B` | **stay in `Escape`** (no dispatch) |
| **EscapeIntermediate** (`lib.rs:393`) | `20-2F` collect; `30-7E` → `esc_dispatch` + `Ground`; `7F` ignored | |
| **CsiEntry** (`lib.rs:188`) | `20-2F` collect → `CsiIntermediate`; `30-39` `paramnext` → `CsiParam`; `3A` (`:`) `subparam` → `CsiParam`; `3B` (`;`) `param` → `CsiParam`; `3C-3F` (`<=>?`) **collect as intermediate** → `CsiParam`; `40-7E` `csi_dispatch` | |
| **CsiParam** (`lib.rs:239`) | `20-2F` collect → `CsiIntermediate`; `30-39` `paramnext`; `3A` `subparam`; `3B` `param`; `3C-3F` → **`CsiIgnore`**; `40-7E` `csi_dispatch`; `7F` ignored | |
| **CsiIntermediate** (`lib.rs:228`) | `20-2F` collect; `30-3F` → `CsiIgnore`; `40-7E` `csi_dispatch` | |
| **CsiIgnore** (`lib.rs:217`) | `20-3F` and `7F` dropped; `40-7E` → `Ground` **with no dispatch** | |
| **DcsEntry/DcsParam/DcsIntermediate** (`lib.rs:257,299,287`) | same param/intermediate rules as CSI, but C0 bytes are **silently dropped, not executed**; `40-7E` → `action_hook` → `DcsPassthrough` | |
| **DcsPassthrough** (`lib.rs:317`) | `00-17,19,1C-7E` → `put(byte)`; `18`/`1A` → `unhook()` + `execute` + `Ground`; `1B` → `unhook()` + `reset_params()` + `Escape`; `7F` ignored; **`9C` → `unhook()` + `Ground`**; everything else dropped | |
| **DcsIgnore**, **SosPmApcString** | routed to `anywhere` (`lib.rs:438`): only `18`/`1A` (`execute`+`Ground`) and `1B` (`reset_params`+`Escape`) escape the state; all payload is discarded | |
| **OscString** (`lib.rs:407`) | `00-06,08-17,19,1C-1F` dropped; `07` → `osc_end(BEL)` + `Ground`; `18`/`1A` → `osc_end` + `execute` + `Ground`; `1B` → `osc_end` + `reset_params()` + **`Escape`**; `3B` (`;`) → new param; anything else → push to `osc_raw` | |

**Consequences to reproduce exactly**

- **Only 7-bit.** 8-bit C1 introducers (`0x9B` CSI, `0x9D` OSC, `0x90` DCS) are *not* recognised.
  They reach `Perform::execute(byte)` (`lib.rs:713-721`, test `c1s` at `lib.rs:1506-1533`) and
  `ansi.rs` then logs them as unhandled. The single exception is `0x9C` inside `DcsPassthrough`.
- `ESC` inside `Escape` is idempotent; `ESC ESC ESC [ A` is one CUU.
- `CSI ? 1 ; 2 ? m` → the second `?` in `CsiParam` sends the whole sequence to `CsiIgnore`, so
  **nothing is dispatched**.
- A `CsiIgnore` final byte produces **no** `csi_dispatch` at all (unlike a param overflow, which
  still dispatches with `ignore = true`).

### 1.3 Parameters, subparameters, defaults

`Params` (`params.rs`) is a flat `[u16; 32]` with a parallel `subparams: [u8; 32]` run-length array.
`push` starts a new parameter, `extend` appends a subparameter to the current one
(`params.rs:62-76`). `ParamsIter` yields `&[u16]` slices — one slice per parameter, its elements
being that parameter's subparameters (`params.rs:100-122`).

| Situation | Behaviour | Cite |
|---|---|---|
| Digit accumulation | `param = param.saturating_mul(10).saturating_add(d)` → clamps at `u16::MAX` | `lib.rs:509-518`; test `parse_long_csi_param` asserts `CSI 9223372036854775808 m` → `[[65535]]` (`lib.rs:1268`) |
| `;` with nothing before it | pushes `0` — `CSI ; 4 m` → `[[0],[4]]`, `CSI 4 ; m` → `[[4],[0]]` | `lib.rs:1099-1127` |
| No params at all | `action_csi_dispatch` still pushes the pending `param` (`0`), so `CSI m` → `[[0]]` and `params.is_empty()` is false | `lib.rs:453-461` |
| 32+ parameters | `params.is_full()` → `ignoring = true`, extra params dropped, dispatch still fires with `ignore = true` | `lib.rs:453-460, 498-507` |
| 32 subparameters | `CSI ::::…:x` (32 colons) → one param `[0; 32]`, `ignore = true` | test `params_buffer_filled_with_subparam`, `lib.rs:1291-1308` |
| `?`/`>`/`=`/`<` | collected as **intermediates**, only from `CsiEntry` (`lib.rs:198-201`) | |
| `reset_params` | clears intermediates, `ignoring`, `param`, `params` — happens on `ESC`, on `ESC [`, on `ESC P`, and on leaving `DcsPassthrough` via `ESC` | `lib.rs:554-561`; tests `csi_reset`, `esc_reset`, `esc_reset_intermediates`, `intermediate_reset_on_dcs_exit` (`lib.rs:1143-1289`) |
| >2 intermediates | `ignoring = true`, extras dropped | `lib.rs:476-485` |

`ansi.rs` layers a default on top (`ansi.rs:1568-1571`):

```rust
let mut next_param_or = |default: u16| match params_iter.next() {
    Some(&[param, ..]) if param != 0 => param,
    _ => default,
};
```

So **a parameter of `0` is treated as absent**, and **only the first subparameter of a parameter is
read** by this helper. `CSI 0 A` == `CSI A` == `move_up(1)`.

### 1.4 OSC

`osc_dispatch` (`lib.rs:569-586`) hands out up to 16 `&[u8]` slices, plus `bell_terminated: bool`.

- The terminator is `BEL` (`0x07`) or `ESC \` (ST). C1 `ST` (`0x9C`) is **not** accepted (removed in
  vte 0.8 so OSC payloads can contain `0x9C` as a UTF-8 continuation byte). Test
  `osc_containing_string_terminator` (`lib.rs:1001-1016`) confirms `\x1b]2;\xe6\x9c\xab\x1b\\`
  keeps the `\x9c` inside the payload.
- `ESC \` termination produces **two** `Perform` calls: `osc_dispatch(…, bell_terminated=false)`
  then `esc_dispatch([], false, b'\\')` — the latter is a no-op in `ansi.rs:1841`. Test
  `osc_c0_st_terminated` asserts `dispatched.len() == 2` (`lib.rs:967`).
- More than 16 `;`-separated params: the 17th onward are dropped (`action_osc_put_param` returns at
  `MAX_OSC_PARAMS`, `lib.rs:520-539`) but the payload bytes still accumulate in `osc_raw`, so the
  16th param swallows the rest.
- With the `std` feature (what OneTerm builds), **`osc_raw` is unbounded** — no 1024-byte cap.
  Test `exceed_max_buffer_size` (`lib.rs:1018-1049`) asserts the std path keeps all
  `MAX_OSC_RAW + 100 + 1` bytes. A reimplementation should impose its own bound but must be aware
  that the reference has none (DoS surface for OSC 52).
- `ESC ] BEL` (empty OSC) still dispatches, with one empty param (`parse_empty_osc`, `lib.rs:920`).
- UTF-8 in OSC payloads is passed through as raw bytes; decoding is the handler's job
  (`parse_osc_with_utf8_arguments`, `lib.rs:984`).

### 1.5 DCS

`hook(params, intermediates, ignore, action)` → zero or more `put(byte)` → `unhook()`
(`lib.rs:761-790`). Notes:

- C0 bytes inside `DcsEntry`/`DcsParam`/`DcsIntermediate` are **dropped**, not executed
  (`lib.rs:258, 288, 300`) — unlike the CSI states.
- Inside `DcsPassthrough`, C0 bytes **are** forwarded to `put` (`lib.rs:318`).
- `DCS` exit via `ESC` calls `unhook()` then `reset_params()`, so the intermediates of the
  *following* escape are clean (test `intermediate_reset_on_dcs_exit`, `lib.rs:1244-1256`).
- Param overflow at hook time works like CSI: `ignore = true`, 32 params kept
  (`parse_dcs_max_params`, `lib.rs:1182-1200`).

### 1.6 UTF-8 handling

`advance_ground` (`lib.rs:590-660`):

1. `memchr(0x1B, bytes)`; if the first byte is `ESC`, short-circuit into `Escape`.
2. `str::from_utf8(&bytes[..plain_chars])`:
   - `Ok` → `ground_dispatch` (print/execute per char), then consume the trailing `ESC` if any.
   - `Err` with `error_len() == Some(len)`:
     - dispatch the valid prefix;
     - if `len == 1 && byte <= 0x9F` → `execute(byte)` (this is how 8-bit C1 reaches `execute`);
     - otherwise → `print('\u{FFFD}')`;
     - **skip `valid_bytes + len` bytes** — the remainder of the invalid run is not re-parsed.
   - `Err` with `error_len() == None` (truncated):
     - if cut off by an `ESC` → `print('\u{FFFD}')` and enter `Escape`
       (test `partial_utf8_into_esc`, `lib.rs:1482-1498`);
     - else buffer into `partial_utf8` and return.

`advance_partial_utf8` (`lib.rs:663-711`) copies up to 4 bytes, and:

- complete → `print(c)`, returns `c.len_utf8() - old_bytes`;
- valid prefix present (a shorter codepoint completed, the rest belongs to the next char) →
  `print` that char and return `valid_bytes - old_bytes` (test `partial_utf8_separating_utf8`,
  `lib.rs:1438-1457`);
- invalid → `print('\u{FFFD}')`;
- still incomplete → consume everything and wait.

Reference tests worth porting verbatim: `unicode`, `invalid_utf8`, `partial_utf8`,
`partial_utf8_separating_utf8`, `partial_invalid_utf8`, `partial_invalid_utf8_split`,
`partial_utf8_into_esc` (`lib.rs:1389-1498`).

### 1.7 `Perform` trait

| Method | Fires on |
|---|---|
| `print(char)` | printable ground character (after UTF-8 assembly) |
| `execute(u8)` | C0 in ground/CSI/ESC states, 8-bit C1 (`0x80..=0x9F`) in ground, `CAN`/`SUB` anywhere |
| `hook(&Params, &[u8], bool, char)` | DCS final byte |
| `put(u8)` | each DCS payload byte |
| `unhook()` | DCS terminated (`ST`, `ESC`, `CAN`, `SUB`, `0x9C`) |
| `osc_dispatch(&[&[u8]], bool)` | OSC terminated |
| `csi_dispatch(&Params, &[u8], bool, char)` | CSI final byte (not in `CsiIgnore`) |
| `esc_dispatch(&[u8], bool, u8)` | ESC final byte |
| `terminated() -> bool` | polled after each byte by `advance_until_terminated` |

### 1.8 `ansi::Handler` — the semantic layer

`Performer` (`ansi.rs:419-…`) implements `Perform` and translates to `Handler`
(`ansi.rs:485-750`). `ProcessorState` keeps `preceding_char` for `CSI b` (REP).

**C0 (`execute`, `ansi.rs:1293-1305`)**

| Byte | Handler call |
|---|---|
| `HT 0x09` | `put_tab(1)` |
| `BS 0x08` | `backspace()` |
| `CR 0x0D` | `carriage_return()` |
| `LF 0x0A`, `VT 0x0B`, `FF 0x0C` | `linefeed()` |
| `BEL 0x07` | `bell()` |
| `SUB 0x1A` | `substitute()` |
| `SI 0x0F` | `set_active_charset(G0)` |
| `SO 0x0E` | `set_active_charset(G1)` |
| anything else | logged, ignored |

Note `newline()` (LF/NL mode) is reachable only from `Term` itself and tests — `execute` always
routes LF to `linefeed()`, and `Term::linefeed` does **not** honour `LINE_FEED_NEW_LINE`. That mode
bit is therefore effectively dead in the data path (`term/mod.rs:1441-1451`, `1428`).

**ESC (`esc_dispatch`, `ansi.rs:1824-1845`)**

| Sequence | Handler |
|---|---|
| `ESC ( B` / `) B` / `* B` / `+ B` | `configure_charset(G0..G3, Ascii)` |
| `ESC ( 0` / `) 0` / `* 0` / `+ 0` | `configure_charset(G0..G3, SpecialCharacterAndLineDrawing)` |
| `ESC D` (IND) | `linefeed()` |
| `ESC E` (NEL) | `linefeed()` then `carriage_return()` |
| `ESC H` (HTS) | `set_horizontal_tabstop()` |
| `ESC M` (RI) | `reverse_index()` |
| `ESC Z` | `identify_terminal(None)` |
| `ESC c` (RIS) | `reset_state()` |
| `ESC 7` (DECSC) | `save_cursor_position()` |
| `ESC 8` (DECRC) | `restore_cursor_position()` |
| `ESC # 8` (DECALN) | `decaln()` |
| `ESC =` / `ESC >` | `set_keypad_application_mode()` / `unset_…` |
| `ESC \` | no-op |
| anything else | logged, ignored |

**CSI (`csi_dispatch`, `ansi.rs:1546-1789`)** — the whole sequence is **dropped** up-front if
`ignore` is set or `intermediates.len() > 2` (`ansi.rs:1563-1566`).

| Sequence | Handler | Default |
|---|---|---|
| `CSI Ps @` | `insert_blank(Ps)` (ICH) | 1 |
| `CSI Ps A` | `move_up` | 1 |
| `CSI Ps B` / `CSI Ps e` | `move_down` | 1 |
| `CSI Ps b` | REP — replays `preceding_char` `Ps` times via `input()` | 1 |
| `CSI Ps C` / `CSI Ps a` | `move_forward` | 1 |
| `CSI Ps D` | `move_backward` | 1 |
| `CSI Ps d` | `goto_line(Ps-1)` (VPA) | 1 |
| `CSI Ps E` / `F` | `move_down_and_cr` / `move_up_and_cr` | 1 |
| `CSI Ps G` / `` CSI Ps ` `` | `goto_col(Ps-1)` (CHA/HPA) | 1 |
| `CSI Ps ; Ps H` / `f` | `goto(row-1, col-1)` | 1;1 |
| `CSI Ps I` / `CSI Ps Z` | `move_forward_tabs` / `move_backward_tabs` | 1 |
| `CSI Ps J` | `clear_screen` — 0 `Below`, 1 `Above`, 2 `All`, 3 `Saved`; other → dropped | 0 |
| `CSI Ps K` | `clear_line` — 0 `Right`, 1 `Left`, 2 `All`; other → dropped | 0 |
| `CSI Ps g` | `clear_tabs` — 0 `Current`, 3 `All`; other → dropped | 0 |
| `CSI ? 5 W` | `set_tabs(8)` | — |
| `CSI Ps L` / `CSI Ps M` | `insert_blank_lines` / `delete_lines` | 1 |
| `CSI Ps P` / `CSI Ps X` | `delete_chars` / `erase_chars` | 1 |
| `CSI Ps S` / `CSI Ps T` | `scroll_up` / `scroll_down` | 1 |
| `CSI Ps ; Ps r` | `set_scrolling_region(top, Option<bottom>)`; `bottom == 0` → `None` | 1, None |
| `CSI s` / `CSI u` | `save_cursor_position` / `restore_cursor_position` | — |
| `CSI Pm h` / `l` | `set_mode` / `unset_mode` **for every parameter** | — |
| `CSI ? Pm h` / `l` | `set_private_mode` / `unset_private_mode` per param; `2026` additionally arms the sync timeout and sets `terminated` | — |
| `CSI Ps $ p` | `report_mode` (DECRQM) | 0 |
| `CSI ? Ps $ p` | `report_private_mode` | 0 |
| `CSI Ps n` | `device_status(Ps)` | 0 |
| `CSI Ps c` (only if `Ps == 0`) | `identify_terminal(intermediate)` — `None` for DA1, `Some('>')` for DA2 | 0 |
| `CSI Ps SP q` | DECSCUSR → `set_cursor_style`; 0→`None`, 1\|2→Block, 3\|4→Underline, 5\|6→Beam; `blinking = Ps % 2 == 1` | 0 |
| `CSI Ps m` | SGR; `params.is_empty()` → `Attr::Reset` | — |
| `CSI > Ps ; Ps m` | `set_modify_other_keys` when first param is `4`: 0 Reset, 1 EnableExceptWellDefined, 2 EnableAll | — |
| `CSI ? 4 m` | `report_modify_other_keys` | — |
| `CSI ? u` | `report_keyboard_mode` (kitty) | — |
| `CSI = Ps ; Ps u` | `set_keyboard_mode(flags, behavior)`; behavior 3 `Difference`, 2 `Union`, else `Replace` | 0, Replace |
| `CSI > Ps u` | `push_keyboard_mode(flags)` | 0 |
| `CSI < Ps u` | `pop_keyboard_modes(Ps)` | **1** |
| `CSI Ps t` | 14 `text_area_size_pixels`, 18 `text_area_size_chars`, 22 `push_title`, 23 `pop_title` | 1 |
| `CSI Ps ; Ps SP k` | SCP: `set_scp(char_path, update_mode)` | 0, 0 |

**Not implemented anywhere in this stack** (silently dropped at `csi_dispatch`'s `_ => unhandled!`):
DECSTR (`CSI ! p`), DECSLRM (`CSI s` is DECSC-style save here, not left/right margins), DECRQSS,
DECSCA / DECSED / DECSEL (`CSI ? Ps J|K`), XTWINOPS other than 14/18/22/23, `CSI Ps SP @`/`A`
(SL/SR), DECCARA/DECERA, `CSI Ps ' }`/`~` (DECIC/DECDC), `CSI Ps SP t`, `CSI Ps SP u`.

**SGR (`attrs_from_sgr_parameters`, `ansi.rs:1856-1938`)**

| Params | `Attr` |
|---|---|
| `0` | `Reset` |
| `1` `2` `3` | `Bold` `Dim` `Italic` |
| `4` (or `4:1`, `4:6`, …) | `Underline` |
| `4:0` | `CancelUnderline` |
| `4:2` `4:3` `4:4` `4:5` | `DoubleUnderline` `Undercurl` `DottedUnderline` `DashedUnderline` |
| `5` `6` | `BlinkSlow` `BlinkFast` |
| `7` `8` `9` | `Reverse` `Hidden` `Strike` |
| `21` | `CancelBold` (**not** double underline) |
| `22` `23` `24` `25` `27` `28` `29` | `CancelBoldDim` `CancelItalic` `CancelUnderline` `CancelBlink` `CancelReverse` `CancelHidden` `CancelStrike` |
| `30-37` / `40-47` / `90-97` / `100-107` | named fg/bg, normal and bright |
| `39` / `49` | `Foreground(Named::Foreground)` / `Background(Named::Background)` |
| `38` / `48` / `58` alone (semicolon form) | consumes **following parameters** via `parse_sgr_color` |
| `38:…` / `48:…` / `58:…` (colon form) | `handle_colon_rgb` on the subparameter tail |
| `59` | `UnderlineColor(None)` |
| anything else | skipped (`None` → `continue`), the rest of the SGR list still processed |

`parse_sgr_color` (`ansi.rs:1951-1961`): `2` → three more values as `Rgb` (each must fit `u8`),
`5` → one value as `Indexed(u8)`, anything else → `None`. **A value > 255 aborts the whole
attribute** (`u8::try_from(...).ok()?`) but the parameters have already been consumed.

`handle_colon_rgb` (`ansi.rs:1942-1948`): given the subparameter tail after `38`,
`rgb_start = if len > 4 { 2 } else { 1 }` — i.e. the ITU colour-space id in
`38:2:<cs>:R:G:B` is skipped, while `38:2:R:G:B` works too. Both `38:5:n` and `38;5;n` yield
`Indexed(n)`; the semicolon form has **no** colour-space variant.

Reproduce these exact asymmetries:

- `CSI 38;2;255;0;255;1 m` sets truecolor fg *and then* bold — `parse_sgr_color` consumed exactly
  three params.
- `CSI 38:2:255:0:255;1 m` — one parameter with subparams, then `1` (test `csi_subparameters`,
  `lib.rs:1162-1179`).
- `CSI 38;5 m` (missing index) → `None` → attribute silently dropped.

**OSC (`osc_dispatch`, `ansi.rs:1327-1544`)**

| OSC | Handler | Notes |
|---|---|---|
| `0`, `2` | `set_title(Some(params[1..].join(";").trim()))` | requires ≥2 params |
| `4` | `set_color(index, rgb)` per `index;spec` pair, or `dynamic_color_sequence("4;{i}", i, term)` when spec is `?` | **requires an odd param count**; even → unhandled |
| `8` | `set_hyperlink(Some(Hyperlink{ id, uri }))`; empty uri → `set_hyperlink(None)` | requires `params.len() > 2`; uri rebuilt by re-joining `params[2..]` with `;`; `id` parsed from `key=value:key=value` in `params[1]` |
| `10`, `11`, `12` | `set_color(256+offset, rgb)` / `dynamic_color_sequence` for `?`; `dynamic_code` increments per extra param, aborting past `Cursor` (258) | |
| `22` | `set_mouse_cursor_icon(CursorIcon)` | exactly 2 params |
| `50` | `set_cursor_shape` when `params[1]` starts `CursorShape=` and the 13th byte is `0`/`1`/`2` → Block/Beam/Underline | |
| `52` | `clipboard_load` when `params[2] == "?"`, else `clipboard_store(params[1][0], base64)` | requires ≥3 params; selection byte defaults to `'c'` |
| `104` | no/empty param → `reset_color(i)` for `i in 0..256`; otherwise per-index | **does not** reset 256/257/258 |
| `110` / `111` / `112` | `reset_color(Foreground/Background/Cursor)` | |
| anything else | **OneTerm fork**: `report_osc(params, bell_terminated)` then logged as unhandled (`ansi.rs:1538-1543`) | covers OSC 1, 7, 9, 133, 777, … |

Colour parsing (`xparse_color`, `ansi.rs:174-222`): accepts `rgb:R/G/B` with 1–4 hex digits per
channel scaled by `255 * v / (16^n - 1)`, and `#RGB`/`#RRGGBB`/`#RRRGGGBBB`/`#RRRRGGGGBBBB`
truncated to the top byte. `parse_number` (`ansi.rs:224-236`) is a `u8` parser that returns `None`
on overflow — so **`OSC 4;321;…` is rejected** (test `parse_number_too_large`).

### 1.9 Synchronized updates (DEC 2026)

Implemented entirely inside `Processor` (`ansi.rs:284-419`), never reaching `Term`.

Constants (`ansi.rs:34-48`): `SYNC_UPDATE_TIMEOUT = 150 ms`, `SYNC_BUFFER_SIZE = 2 MiB`,
`SYNC_ESCAPE_LEN = 8`, `BSU_CSI = b"\x1b[?2026h"`, `ESU_CSI = b"\x1b[?2026l"`.

Flow:

1. `csi_dispatch` sees `?2026` in a `h` sequence → `timeout.set_timeout(150ms)` and
   `self.terminated = true` (`ansi.rs:1702-1710`). `advance_until_terminated` returns immediately
   after that byte.
2. `Processor::advance` loops; `pending_timeout()` is now true → all remaining bytes go to
   `advance_sync` (`ansi.rs:367-386`): appended to `sync_state.buffer`, **not parsed**.
3. `advance_sync_csi` (`ansi.rs:388-416`) scans only the newly added window (plus 7 bytes of
   overlap) for `0x1B`, **in reverse**, and compares the 8 bytes at each hit against `BSU_CSI` /
   `ESU_CSI` byte-for-byte. Only *exactly* those sequences count — `\x1b[?2026;1h` does not.
   - BSU hit → refresh the timeout, remember `bsu_offset`.
   - ESU hit → `stop_sync_internal(handler, bsu_offset)` and stop scanning.
4. `stop_sync_internal` (`ansi.rs:322-357`) parses `buffer[..offset]` with plain `advance`
   (so nested BSUs inside the flush do not re-arm), then either keeps the tail from `bsu_offset`
   (a new update started before the old one ended) or calls
   `handler.unset_private_mode(SyncUpdate)`, clears the timeout and empties the buffer.
5. Overflow guard: if `buffer.len() + bytes.len() >= SYNC_BUFFER_SIZE - 1`, the update is force-ended
   and the bytes parsed normally (`ansi.rs:375-380`).
6. **Timeout expiry is the embedder's responsibility.** `StdSyncHandler::pending_timeout` merely
   checks `Option::is_some` (`ansi.rs:462-465`); the host must poll `Processor::sync_timeout()` and
   call `Processor::stop_sync(handler)` when the `Instant` passes, otherwise the screen freezes
   forever.

**`DCS = 1 s` / `= 2 s` batching is _not_ implemented.** That byte sequence parses as a DCS with
intermediate `=`, param `1`, final `s` → `Handler::dcs_hook` (OneTerm patch 0002), and `Term` drops
any DCS whose final byte isn't `q` (`term/mod.rs:1214-1217`). A reimplementation is free to add it
but must not assume the reference does.

---

## 2. `Term` — the screen model

### 2.1 `TermMode` bits

`term/mod.rs:59-92`. Default is `SHOW_CURSOR | LINE_WRAP | ALTERNATE_SCROLL | URGENCY_HINTS`
(`term/mod.rs:118-125`).

| Bit | Set by | Effect inside the engine |
|---|---|---|
| `SHOW_CURSOR` (1) | `CSI ?25h/l` | `RenderableCursor::shape` becomes `Hidden` when clear and not in vi mode (`term/mod.rs:2469-2473`) |
| `APP_CURSOR` (1<<1) | `CSI ?1h/l` | pure reporting flag; key encoding lives in the embedder |
| `APP_KEYPAD` (1<<2) | `ESC =` / `ESC >` | same |
| `MOUSE_REPORT_CLICK` (1<<3) | `CSI ?1000h` | reporting flag |
| `BRACKETED_PASTE` (1<<4) | `CSI ?2004h` | reporting flag (embedder wraps pastes in `ESC[200~`/`ESC[201~`) |
| `SGR_MOUSE` (1<<5) | `CSI ?1006h` | mutually exclusive with `UTF8_MOUSE` **on set only** |
| `MOUSE_MOTION` (1<<6) | `CSI ?1003h` | |
| `LINE_WRAP` (1<<7) | `CSI ?7h/l` | gates `wrapline()` entirely, and changes wide-char-at-last-column handling |
| `LINE_FEED_NEW_LINE` (1<<8) | `CSI 20h/l` | only consulted by `Term::newline`, which nothing in the data path calls (see §1.8) |
| `ORIGIN` (1<<9) | `CSI ?6h/l` | `goto` offsets by `scroll_region.start` and clamps to `scroll_region.end - 1`; setting it also does `goto(0,0)` |
| `INSERT` (1<<10) | `CSI 4h/l` | `input()` shifts the row right; **`Term::damage()` marks the terminal fully damaged every frame while set** (`term/mod.rs:455-459`) |
| `FOCUS_IN_OUT` (1<<11) | `CSI ?1004h` | reporting flag |
| `ALT_SCREEN` (1<<12) | `swap_alt` | selects which grid is active |
| `MOUSE_DRAG` (1<<13) | `CSI ?1002h` | |
| `UTF8_MOUSE` (1<<14) | `CSI ?1005h` | |
| `ALTERNATE_SCROLL` (1<<15) | `CSI ?1007h` | reporting flag |
| `VI` (1<<16) | `Term::toggle_vi_mode` | selects vi cursor for rendering; survives `reset_state` |
| `URGENCY_HINTS` (1<<17) | `CSI ?1042h` | reporting flag |
| `DISAMBIGUATE_ESC_CODES` (1<<18) … `REPORT_ASSOCIATED_TEXT` (1<<22) | kitty keyboard | five bits mirroring `KeyboardModes` |
| `MOUSE_MODE` | alias for CLICK\|MOTION\|DRAG | cleared before setting any mouse mode |
| `KITTY_KEYBOARD_PROTOCOL` | alias for the five kitty bits | |

Mouse-mode exclusivity is **asymmetric**: `set_private_mode` clears `MOUSE_MODE` before inserting
the new bit (`term/mod.rs:1979-1994`), but `unset_private_mode` removes only the one bit
(`term/mod.rs:2044-2056`).

### 2.2 Cursor

`grid::Cursor<T>` (`grid/mod.rs:33-53`) holds `point: Point`, `template: Cell`, `charsets:
Charsets` (G0–G3), and `input_needs_wrap: bool`. Each `Grid` owns a `cursor` **and** a
`saved_cursor`, so the primary and alternate screens have independent DECSC slots.

`input_needs_wrap` is the *pending wrap* flag. It is set when a glyph lands on the last column and
cleared by every explicit positioning operation: `goto` (`term/mod.rs:1183`), `move_forward`,
`move_backward`, `carriage_return`, `backspace`, `wrapline`. It is **not** cleared by `move_up`,
`move_down` (they route through `goto`, so they do clear it), by `linefeed`, or by
`reverse_index`.

`active_charset` (which of G0–G3 is invoked by `SI`/`SO`) lives on **`Term`, not the cursor**
(`term/mod.rs:292`), so `DECSC`/`DECRC` do not save/restore it and it is not swapped with the alt
screen. The *designations* (`charsets[G0..G3]`) live on the cursor and therefore are.

### 2.3 `input()` — printing a character

`term/mod.rs:1077-1163`.

1. `c.width()`; `None` (control/unassigned) → return.
2. **width 0** — take `column = cursor.column`; if `!input_needs_wrap` then `column -= 1`
   (saturating); if that cell is a `WIDE_CHAR_SPACER`, step back once more; then
   `grid[line][column].push_zerowidth(c)` into `CellExtra::zerowidth` (unbounded `Vec<char>`,
   `term/cell.rs:127, 166-170`). At column 0 with no pending wrap this writes onto column 0.
3. If `input_needs_wrap` → `wrapline()` (§2.4).
4. **Insert mode** — only if `cursor.column + width < columns`: swap `row[col + width] ↔ row[col]`
   for `col` descending over `col.0 .. columns - width` (`term/mod.rs:1104-1113`). Wide-char pairs
   are **not** repaired, so inserting over a wide char can leave an orphaned spacer.
5. **width 1** → `write_at_cursor(c)`.
6. **width 2**:
   - if `cursor.column + 1 >= columns`:
     - `LINE_WRAP` set → write `' '` with `LEADING_WIDE_CHAR_SPACER` into the last column, then
       `wrapline()`;
     - `LINE_WRAP` clear → `input_needs_wrap = true` and **the character is dropped**.
   - write the glyph with `WIDE_CHAR`, advance one column, write `' '` with `WIDE_CHAR_SPACER`.
7. Finally `if column + 1 < columns { column += 1 } else { input_needs_wrap = true }`.

`write_at_cursor` (`term/mod.rs:997-1027`) maps `c` through `cursor.charsets[active_charset]`,
copies fg/bg/flags/extra from `cursor.template`, and **repairs the wide pair it is overwriting**:

- overwriting a `WIDE_CHAR` at `column < last_column` → clear `WIDE_CHAR_SPACER` at `column + 1`;
- otherwise if `column > 0` → `grid[line][column-1].clear_wide()` (drops `WIDE_CHAR`, wipes
  zerowidth, sets `c = ' '`);
- if `column <= 1` and not on the topmost line → clear `LEADING_WIDE_CHAR_SPACER` from the previous
  line's last column.

### 2.4 `wrapline()` and the `WRAPLINE` flag

`term/mod.rs:968-991`:

```
if !LINE_WRAP { return }                     // pending wrap stays set forever
cursor_cell().flags |= WRAPLINE
if cursor.line + 1 >= scroll_region.end { linefeed() } else { line += 1 }
cursor.column = 0; input_needs_wrap = false
```

`WRAPLINE` on the last cell of a row is the *only* record that a logical line continues, and it
drives: reflow (`grid/resize.rs:105, 132, 204-207, 304-318`), `LineLength::line_length`
(`term/cell.rs:300-302`), `line_to_string` newline insertion (`term/mod.rs:629-635`),
`line_search_left/right`, `inline_search_left/right` (`term/search.rs:538-611`).

### 2.5 Cursor motion

| Op | Rule | Cite |
|---|---|---|
| `goto(line, col)` | origin mode → `y_offset = scroll_region.start`, `max_y = scroll_region.end - 1`; else `(0, bottommost_line)`. `line = clamp(line + y_offset, 0, max_y)`, `col = min(col, last_column)`. Damages both old and new position. Clears pending wrap. | `term/mod.rs:1167-1184` |
| `move_up/down` | delegate to `goto` (so they clamp to the region in origin mode) | `1230-1245` |
| `move_forward/backward` | clamp to `last_column` / `saturating_sub`; **do not** go through `goto`, so they ignore origin mode; clear pending wrap | `1247-1268` |
| `backspace` | only when `column > 0`; `column -= 1`; clears pending wrap. **No reverse wrap** — `BS` at column 0 is a no-op | `1410-1420` |
| `carriage_return` | `column = 0`, clears pending wrap | `1423-1431` |
| `linefeed` | `next = line + 1`; if `next == scroll_region.end` → `scroll_up(1)`; else if `next < screen_lines` → `line += 1`; else nothing. So a cursor *below* the region just walks down to the screen bottom | `1434-1444` |
| `reverse_index` | if `line == scroll_region.start` → `scroll_down(1)`; else `line = max(line - 1, 0)` | `term/mod.rs:1878-1888` |
| `put_tab(n)` | if pending wrap → `wrapline()` and return; else per count: write `'\t'` (charset-mapped) into the cell **only if the cell currently holds `' '`**, then walk right to the next tab stop, stopping at `columns - 1` | `1394-1407` |
| `move_forward_tabs` / `move_backward_tabs` | walk the `tabs` bitmap; forward stops at `num_cols - 1`, backward stops at column 0 | `1585-1621` |

### 2.6 Scroll region (DECSTBM)

`set_scrolling_region(top, bottom)` (`term/mod.rs:2178-2202`):

- `bottom` defaults to `screen_lines()`;
- `if top >= bottom { return }` — invalid regions are a no-op, the old region survives;
- `start = Line(top - 1)`, `end = Line(bottom)`, both `min`-clamped to `screen_lines`;
- ends with `goto(0, 0)` — which is origin-relative, so `DECSTBM` always homes the cursor.

Because `next_param_or(1)` maps `0 → 1`, `top` from a CSI can never be 0. The region is *not*
validated against the grid height beyond the `min`, and `Term::resize` unconditionally resets it to
the full screen (`term/mod.rs:713`).

`scroll_up_relative(origin, lines)` / `scroll_down_relative` (`term/mod.rs:759-804`) clamp `lines`
to the region height, rotate the selection, move the vi cursor, call `Grid::scroll_up/down`, and
`mark_fully_damaged()`.

- `scroll_up(n)` (SU / `CSI S`) and `linefeed`-induced scroll use `origin = scroll_region.start`.
- `insert_blank_lines` (IL, `CSI L`) uses `origin = cursor.line` and is a **no-op when the cursor is
  outside the region** (`term/mod.rs:1496-1504`).
- `delete_lines` (DL, `CSI M`) clamps `lines` to `screen_lines - cursor.line` and is likewise a
  no-op outside the region (`term/mod.rs:1506-1515`).

### 2.7 Erase operations

| Op | Range | Details | Cite |
|---|---|---|---|
| `EL 0` (Right) | `cursor.column .. columns` | **returns immediately if `input_needs_wrap`** | `term/mod.rs:1651-1673` |
| `EL 1` (Left) | `0 .. cursor.column + 1` | inclusive of the cursor cell | same |
| `EL 2` (All) | `0 .. columns` | | same |
| `ECH` (`CSI X`) | `col .. min(col + n, columns)` | fills with `cursor.template.bg` | `1528-1543` |
| `DCH` (`CSI P`) | `n = min(count, columns)`; `start = col`, `end = min(start + n, columns - 1)`; swap `row[start+o] ↔ row[end+o]` for `o in 0..(columns - end)`; then clear `row[columns - n ..]` | note the `columns - 1` clamp on `end` — a quirk that makes large `n` behave unlike a plain "shift left by n" | `1546-1572` |
| `ICH` (`CSI @`) | `n = min(count, columns - col)`; swap from the end; fill `col .. col + n` with bg | | `1186-1211` |
| `ED 0` (Below) | clear `cursor.column..` on the cursor line, then `reset_region((line+1)..)` | | `1780-1792` |
| `ED 1` (Above) | **`if cursor.line > 1`** → `reset_region(..cursor.line)`; then clear `0 .. cursor.column + 1` on the cursor line | the `> 1` guard means line 0 is *not* cleared when the cursor is on line 1 | `1766-1779` |
| `ED 2` (All) | alt screen → `reset_region(..)`. Primary → **`grid.clear_viewport()`**, which scrolls the occupied part of the viewport into scrollback rather than discarding it; the vi cursor is pulled down by the scrolled amount; `display_offset` is preserved | `1793-1809` |
| `ED 3` (Saved) | only when `history_size() > 0` → `grid.clear_history()` (which also sets `display_offset = 0`); vi cursor clamped with `Boundary::Cursor` | `1810-1818` |

All four `ED` modes end with `mark_fully_damaged()`. `ED 2` and `ED 3` additionally emit
**`Event::ClearScreen`** (OneTerm patch 0002) — computed *before* the `history_size() > 0` guard, so
`CSI 3 J` with no scrollback still fires the event while changing nothing
(`term/mod.rs:1765, 1834-1837`).

Selection invalidation: `EL*` drops a selection intersecting the cursor line; `ED 0`/`ED 1` drop one
intersecting the cleared range; `ED 2` sets `selection = None`; `ED 3` drops one intersecting
`..Line(0)`.

### 2.8 Tab stops

`TabStops::new(columns)` → `i % 8 == 0` (`INITIAL_TABSTOPS = 8`, `term/mod.rs:56, 2416-2419`).

- `HTS` (`ESC H`) sets a stop at the cursor column.
- `TBC 0` clears the stop under the cursor; `TBC 3` clears all (`clear_all`, `term/mod.rs:2422-2427`).
- `CSI ? 5 W` calls `Handler::set_tabs(8)` — **`Term` does not implement it**, so it is a no-op
  (the default body in `ansi.rs:615` wins). There is therefore *no* way to restore default stops
  short of `RIS`.
- `TabStops::resize(columns)` (`term/mod.rs:2430-2438`) uses `resize_with` starting `index` at the
  old length, so new columns get stops at absolute multiples of 8, while stops that were cleared in
  the retained prefix stay cleared. Shrinking truncates; growing back re-creates *default* stops in
  the regrown region.

### 2.9 Alternate screen

`swap_alt` (`term/mod.rs:734-756`):

```
if !ALT_SCREEN {                              // entering
    inactive_grid.cursor = grid.cursor.clone()    // alt starts at the primary cursor
    grid.saved_cursor  = grid.cursor.clone()      // *clobbers* the primary DECSC slot
    inactive_grid.reset_region(..)                // alt screen wiped
}
swap(keyboard_mode_stack, inactive_keyboard_mode_stack)
set_keyboard_mode(top-of-new-stack, Replace)
swap(grid, inactive_grid)
mode ^= ALT_SCREEN
selection = None
mark_fully_damaged()
```

Leaving alt takes none of the `if` branch, so the primary screen comes back with exactly the cursor
and `saved_cursor` it had on entry.

**Only `CSI ? 1049 h/l` reaches this.** `PrivateMode::new` (`ansi.rs:895-916`) maps 1049 only;
`47`, `1047` and `1048` become `PrivateMode::Unknown` and `Term::set_private_mode` logs and returns
(`term/mod.rs:1944-1951`). The alternate grid is created with `max_scroll_limit = 0`
(`term/mod.rs:419`), so it has no scrollback ever.

### 2.10 Reset

`reset_state` (RIS, `ESC c`) — `term/mod.rs:1841-1875`:

- if on alt, swap the grids back;
- `active_charset = G0`, `cursor_style = None`, Sixel state dropped;
- `grid.reset()` **and** `inactive_grid.reset()` (clears history, cursors, `display_offset`, all
  visible rows);
- `scroll_region` → full screen, `tabs` → fresh, `title = None`, `title_stack` empty,
  `selection = None`, vi cursor default, both keyboard-mode stacks emptied;
- `mode &= VI; mode |= TermMode::default()` — **vi mode survives RIS**;
- emits `CursorBlinkingChange` and (OneTerm) `ClearScreen`, then full damage.

`reset_state` does **not** clear `self.colors` — OSC 4/10/11/12 overrides survive RIS. It also does
not reset `is_focused` or the `Config`.

**DECSTR (`CSI ! p`) is not implemented at all** — see §1.8.

### 2.11 Colours

`Colors` is `[Option<Rgb>; 269]` (`term/color.rs:6-27`); `None` means "use the theme value".
Layout (`term/color.rs:9-20` — note the doc comment's `233..256` and `268 = dim background` are both
off; the enum is authoritative):

| Index | Meaning |
|---|---|
| 0..16 | named ANSI (`NamedColor::Black` … `BrightWhite`) |
| 16..232 | 6×6×6 colour cube |
| 232..256 | 24-step grayscale ramp |
| 256 | `Foreground` |
| 257 | `Background` |
| 258 | `Cursor` |
| 259..267 | `DimBlack` … `DimWhite` |
| 267 | `BrightForeground` |
| 268 | `DimForeground` |

`Cell::fg`/`bg` store `ansi::Color` = `Named(NamedColor) | Spec(Rgb) | Indexed(u8)`
(`ansi.rs:1147-1153`). Bright/dim *mapping* (`NamedColor::to_bright` / `to_dim`,
`ansi.rs:1092-1140`) is not applied by `Term` — the renderer decides whether `BOLD` promotes
`Named(Red)` to `BrightRed` and whether `DIM` demotes it.

`set_color` / `reset_color` (`term/mod.rs:1676-1718`) mark the terminal fully damaged unless the
index is `Cursor` **and** the value actually changed.

`dynamic_color_sequence` emits `Event::ColorRequest(index, formatter)`; the formatter produces
`\x1b]{prefix};rgb:{rr}/{gg}/{bb}{terminator}` with each channel doubled
(`term/mod.rs:1690-1704`).

### 2.12 Title stack, bell, DA, DSR, DECRPM

| Feature | Behaviour | Cite |
|---|---|---|
| Title | `set_title` stores and emits `Event::Title` / `Event::ResetTitle` | `term/mod.rs:2251-2262` |
| `CSI 22 t` | push current title; stack capped at `TITLE_STACK_MAX_DEPTH = 4096`, overflow removes the **bottom** element | `term/mod.rs:47, 2321-2331` |
| `CSI 23 t` | pop and apply (`None` restores the default title) | `2334-2341` |
| Bell | `Event::Bell` | `1447-1450` |
| DA1 (`CSI c`) | **OneTerm**: `\x1b[?62;4c` (VT220 + Sixel). Upstream alacritty answers `\x1b[?6c` | `1272-1279` |
| DA2 (`CSI > c`) | `\x1b[>0;{version};1c`, `version = version_number(CARGO_PKG_VERSION)` where `1.2.3 → 10203` | `1281-1286`, `2391-2403` |
| DSR 5 | `\x1b[0n` | `1352-1355` |
| DSR 6 (CPR) | `\x1b[{line+1};{col+1}R` — **absolute, ignores origin mode** | `1356-1360` |
| DECRQM `CSI Ps $ p` | `\x1b[{raw};{state}$y` | `2168-2176` |
| DECRQM `CSI ? Ps $ p` | `\x1b[?{raw};{state}$y` | `2114-2124` |
| `ModeState` | `NotSupported = 0`, `Set = 1`, `Reset = 2`; `SyncUpdate` always reports `Reset`, `ColumnMode` always `NotSupported`, unknown private modes `NotSupported` | `2106-2113, 2304-2318` |
| `CSI 18 t` | `\x1b[8;{lines};{cols}t` | `2343-2347` |
| `CSI 14 t` | `Event::TextAreaSizeRequest` → `\x1b[4;{height};{width}t` from the embedder's cell metrics | `2334-2341` |

### 2.13 Keyboard modes

Kitty protocol (`term/mod.rs:1290-1345`), all gated on `config.kitty_keyboard`:

- `CSI > Ps u` → push onto `keyboard_mode_stack` (cap `KEYBOARD_MODE_STACK_MAX_DEPTH = 4096`;
  **the overflow path has a bug — it pops from `title_stack`, not `keyboard_mode_stack`**,
  `term/mod.rs:1305-1311`), then apply with `Replace`.
- `CSI < Ps u` → `truncate(len - Ps)` (default `Ps = 1`), reload the new top or `NO_MODE`.
- `CSI = Ps ; Pb u` → apply with `Replace` / `Union` (`Pb == 2`) / `Difference` (`Pb == 3`).
- `CSI ? u` → `\x1b[?{bits}u` from the **top of the stack**, not from the active mode bits.
- The stack is swapped with `inactive_keyboard_mode_stack` on `swap_alt`.
- `Term::set_options` clears both stacks and `KITTY_KEYBOARD_PROTOCOL` if `config.kitty_keyboard`
  changed (`term/mod.rs:524-528`).

`modifyOtherKeys` (`CSI > 4 ; Ps m`, `CSI ? 4 m`): parsed by vte into
`Handler::set_modify_other_keys` / `report_modify_other_keys`, but **`Term` implements neither** —
they are no-ops. The same applies to `set_scp` and `set_mouse_cursor_icon` (OSC 22).

### 2.14 Hyperlinks (OSC 8)

`set_hyperlink` writes into `cursor.template.extra.hyperlink` (`term/mod.rs:1841-1845`), so every
subsequently written cell carries an `Arc<HyperlinkInner>`. Hyperlinks without an explicit `id=`
get a synthesized `"{counter}_alacritty"` id from a process-global `AtomicU32`
(`term/cell.rs:41, 86-98`) — meaning two independently opened links are never treated as the same
link, and the counter is shared across all `Term` instances in the process.

### 2.15 Damage tracking

`LineDamageBounds { line, left, right }` (`term/mod.rs:136-175`), one per **viewport** line.
`undamaged` is `left = num_cols, right = 0`; `is_damaged()` is `left <= right`.

`TermDamageState` (`term/mod.rs:223-270`) also carries a `full: bool` hint and `last_cursor`.
It starts `full = true` (`term/mod.rs:237`).

`Term::damage()` (`term/mod.rs:453-486`):

1. if `INSERT` mode → `mark_fully_damaged()` **every call**;
2. swap in the new cursor point; if it moved, damage the old point;
3. always damage the current cursor;
4. if `full` → `TermDamage::Full`, else `TermDamage::Partial(TermDamageIterator)`.

`TermDamageIterator::new` (`term/mod.rs:190-198`) truncates the slice to
`num_lines - display_offset` and reports `line + display_offset` — so while scrolled back, only the
first `screen_lines - display_offset` rows can be reported and their indices are shifted.

**Full-damage triggers** (each calls `mark_fully_damaged`):
`scroll_display` when `display_offset` actually changed; `set_options`; `swap_alt`;
`scroll_up_relative` / `scroll_down_relative` (so every `linefeed` that scrolls); `deccolm`;
`decaln`; every `clear_screen` mode; `set_color` / `reset_color` for a non-cursor index whose value
changed; `unset_mode(Insert)`; `Term::resize`; `reset_state`; and `damage()` itself while `INSERT`
is set. The reference test `full_damage` (`term/mod.rs:3252-3335`) enumerates these; port it.

Per-line damage is recorded by: `goto`, `move_forward`, `move_backward`, `backspace`,
`carriage_return`, `linefeed`, `wrapline`, `restore_cursor_position`, `erase_chars`,
`delete_chars` (whole line), `insert_blank` (whole line), `clear_line`, `move_*_tabs`,
`reverse_index`, and `dcs_unhook` for Sixel rows. **Selection and the vi cursor are explicitly not
part of damage** — the embedder diffs those itself (`term/mod.rs:444-449`).

### 2.16 Resize

`Term::resize(size)` (`term/mod.rs:668-716`):

1. early-return if both dimensions are unchanged;
2. compute a vi-cursor `delta = clamp(num_lines - old_lines, min_delta, history_size)` where
   `min_delta = min(0, num_lines - cursor.line - 1)`; apply to `vi_mode_cursor.point.line`;
3. `grid.resize(!is_alt, lines, cols)` and `inactive_grid.resize(is_alt, lines, cols)` — **reflow is
   enabled only for the primary grid**, whichever side it is currently on;
4. if the column count changed → `selection = None` and `tabs.resize(num_cols)`;
   else → `selection.rotate(self, Line(0)..Line(max(new, old) lines), -delta)`;
5. clamp the vi cursor to the viewport;
6. `scroll_region = Line(0)..Line(screen_lines)` — **DECSTBM is always destroyed by a resize**;
7. `damage.resize(...)` → full damage.

`Grid::resize` (`grid/resize.rs:14-36`) temporarily swaps out `cursor.template` for `T::default()`
so newly created cells never inherit the current background, then does **lines first, columns
second**.

**`grow_lines`** (`grid/resize.rs:43-69`): pull `from_history = min(history_size, lines_added)` rows
out of scrollback; for whatever couldn't be pulled, `scroll_up(full region, delta)` so content moves
up and blank rows appear at the bottom; `cursor.line += from_history` and
`saved_cursor.line += from_history`; `display_offset -= lines_added` (saturating);
`decrease_scroll_limit(lines_added)`.

**`shrink_lines`** (`grid/resize.rs:78-98`): `required_scrolling = (cursor.line + 1) - target`;
if positive, `scroll_up(full region, required_scrolling)` (pushing content into history) and clamp
the cursor; clamp `saved_cursor.line`; `raw.rotate((lines - target) as isize)` then
`shrink_visible_lines(target)`.

**`grow_columns`** (`grid/resize.rs:101-242`) — join wrapped lines:

- iterate rows bottom-up into a `reversed` buffer;
- a row is a reflow target when `reflow && row.len() < columns && row[len-1]` has `WRAPLINE`;
- before appending, drop the target's trailing `LEADING_WIDE_CHAR_SPACER` and clear its `WRAPLINE`;
- pull `min(row.len(), columns - last_len)` cells off the **front** of the row below
  (`front_split_off`), inserting a fresh `LEADING_WIDE_CHAR_SPACER` when the boundary would split a
  `WIDE_CHAR`;
- if the donor row became clear, drop it and, when it was above the viewport,
  `display_offset -= 1`; when it was above the cursor, `cursor.line += 1`;
- re-set `WRAPLINE` on the target if the donor still has content;
- the cursor is reflowed through `Point::sub(.., Boundary::Cursor, num_wrapped)`; if nothing
  reflowed with it and the row is clear, `input_needs_wrap` is re-armed;
- `input_needs_wrap` is temporarily converted into `cursor.column += 1` (a column *equal to*
  `columns`) at the start so the special case doesn't have to be threaded through
  (`grid/resize.rs:114-117`);
- afterwards the buffer is padded to at least `lines` rows, `cursor_line_delta` is settled, every
  row is `grow`n to `columns`, and `display_offset = min(display_offset, history_size())`.

**`shrink_columns`** (`grid/resize.rs:245-388`) — split lines:

- iterate rows bottom-up; `row.shrink(columns)` returns the overflow cells (trailing empty cells
  trimmed, `grid/row.rs:71-87`);
- if a `WIDE_CHAR` would land in the new last column, replace it with a
  `LEADING_WIDE_CHAR_SPACER` and prepend the wide char to the wrapped part;
- if the wrapped part ends in a `LEADING_WIDE_CHAR_SPACER`, drop it and move `WRAPLINE` one cell
  left (or, when it is the only cell, keep the row and set `WRAPLINE`);
- set `WRAPLINE` on the row that was cut;
- if the wrapped remainder itself ends with `WRAPLINE` and is shorter than `columns`, buffer it to
  be prepended to the **next** row up (`append_front`), otherwise emit it as a new row and
  `display_offset += 1` if it was above the viewport;
- the cursor moves by `cursor.line -= 1` for every new row created at or below it, and
  `cursor.column -= columns` when it was beyond the new width;
- final: `reversed.truncate(max_scroll_limit + lines)` — **history beyond the limit is discarded**;
  `display_offset = min(display_offset, history_size())`;
- cursor fix-up (`grid/resize.rs:374-387`): with reflow off, clamp the column; with reflow on, if
  `column == columns` and the last cell has no `WRAPLINE`, re-arm `input_needs_wrap` and step back;
  else `grid_clamp(Boundary::Cursor)`. `saved_cursor.column` is only clamped, never reflowed.

Reference tests to port verbatim: `shrink_reflow`, `shrink_reflow_twice`,
`shrink_reflow_empty_cell_inside_line`, `grow_reflow`, `grow_reflow_multiline`,
`grow_reflow_disabled`, `shrink_reflow_disabled` (`grid/tests.rs:164-349`), and
`grow_/shrink_lines_updates_{active,inactive}_cursor_pos` (`term/mod.rs:2988-3078`).

### 2.17 `renderable_content`

`RenderableContent` (`term/mod.rs:2481-2503`) is the whole render contract:

| Field | Source |
|---|---|
| `display_iter: GridIterator<Cell>` | `grid.display_iter()` — exactly the visible rows, cell by cell, each `Indexed { point, cell }` |
| `selection: Option<SelectionRange>` | `selection.to_range(term)` |
| `cursor: RenderableCursor` | see below |
| `display_offset: usize` | `grid.display_offset()` |
| `colors: &Colors` | the 269 overrides |
| `mode: TermMode` | copy of the mode bits |

`RenderableCursor::new` (`term/mod.rs:2461-2477`): point is the vi cursor when `VI` is set, else
`grid.cursor.point`; if that cell is a `WIDE_CHAR_SPACER` the column is decremented; shape is
`CursorShape::Hidden` when not in vi mode and `SHOW_CURSOR` is clear, otherwise
`term.cursor_style().shape`. `cursor_style()` (`term/mod.rs:953-963`) is
`self.cursor_style.unwrap_or(config.default_cursor_style)`, overridden by
`config.vi_mode_cursor_style` while in vi mode.

Note the cursor `point.line` is a grid line (0-based from the top of the *viewport*, so it can be
positive while `display_offset > 0` puts it off-screen). The renderer must add `display_offset`.

### 2.18 Selection

`SelectionType` (`selection.rs:92-98`): `Simple`, `Block`, `Semantic`, `Lines`.

- `Selection::new(ty, point, side)` then `update(point, side)`; `Side = Direction::{Left,Right}`.
- `is_empty()` (`selection.rs:193-225`): `Simple` is empty when the anchors coincide or when they
  are adjacent with `Right → Left` sides; `Block` has an analogous column-only rule; `Semantic` and
  `Lines` are **never** empty.
- `to_range(term)` (`selection.rs:271-296`): orders the anchors, returns `None` if the end is above
  `topmost_line()`, `grid_clamp`s both with `Boundary::Grid`, then dispatches:
  - `Simple` → `range_simple` — drops the last cell when the end side is `Left` (wrapping to the
    previous row's last column when the end column is 0) and drops the first cell when the start
    side is `Right` (wrapping forward).
  - `Block` → `range_block` — normalises to top-left→bottom-right by swapping **columns and sides**
    only; sets `is_block = true`.
  - `Semantic` → if start == end, try `bracket_search` first (pairs `()[]{}<>`,
    `term/search.rs:19, 472-512`); otherwise `semantic_search_left/right`, which use
    `config.semantic_escape_chars` (default `",│`|:\"' ()[]{}<>\t"`, `term/mod.rs:50`) and skip
    wide-char spacers. The searches stop at a row boundary that lacks `WRAPLINE`.
  - `Lines` → `line_search_left/right`, which walk across `WRAPLINE` continuations and then snap to
    column 0 / `last_column`.
- `SelectionRange::contains_cell` (`selection.rs:60-88`) refuses to invert a `Block`-shaped cursor
  cell sitting exactly on a selection corner, and extends a `WIDE_CHAR` cell's membership to its
  spacer.
- `rotate(dimensions, range, delta)` (`selection.rs:137-191`) moves both anchors by `-delta`,
  clamping to the region and returning `None` when the selection collapses. The special case
  `range_top == 0` makes selections in *history* rotate too.
- `include_all()` forces the sides outward (used by vi mode).

`Term::selection_to_string` / `bounds_to_string` / `line_to_string`
(`term/mod.rs:527-645`) define the copy format: tab runs are collapsed to the next tab stop,
`WIDE_CHAR_SPACER` and `LEADING_WIDE_CHAR_SPACER` cells are skipped, zero-width chars are appended
after their base char, and a `\n` is appended when the selection reaches the last column of a row
whose last cell lacks `WRAPLINE`.

### 2.19 Sixel graphics (OneTerm patch 0003)

`Term::dcs_hook` keeps a `SixelParser` only for final byte `'q'`; everything else is dropped
(`term/mod.rs:1214-1217`). `dcs_put` streams bytes into it. `dcs_unhook`
(`term/mod.rs:1232-1262`) lays the image out:

- pixels are measured in `VIRTUAL_CELL = (10, 20)` (`term/graphics.rs:22`) — the VT240/VT340 and
  conhost value, *not* the real cell size;
- `cols = min(ceil(width/10), columns - cursor.column)`, `rows = ceil(height/20)`;
- every covered cell gets `Cell::set_graphic(Some(GraphicCell { id, col, row }))` stored in
  `CellExtra::graphic`;
- the cursor walks down through `linefeed()` for `cursor_rows` rows (so the scroll region and
  scrollback apply) and keeps its column; rows below that are placed without moving the cursor and
  clipped at the screen bottom;
- pixels queue in `Graphics::pending` until `Term::take_graphics()` drains them
  (`term/mod.rs:456-460`); `reset_state` discards them.

`MAX_DIMENSION = 4096` (`term/graphics.rs:17`). The decoder covers the DEC STD 070 subset: raster
attributes `"Pan;Pad;Ph;Pv`, colour registers in RGB (`Pu = 2`) and HLS (`Pu = 1`), `!` repeat,
`$` carriage return, `-` new band; untouched pixels stay transparent regardless of P2.

(The `vendor/README.md` mentions a `Term::set_cell_size()` — it does not exist in the tree; treat
that line as stale.)

---

## 3. Grid storage

### 3.1 `Storage<T>` — the ring buffer

`grid/storage.rs:33-53`:

| Field | Meaning |
|---|---|
| `inner: Vec<Row<T>>` | backing allocation; may be **larger** than `len` (a row cache) |
| `zero: usize` | index in `inner` of the **bottommost** terminal line |
| `visible_lines: usize` | viewport height |
| `len: usize` | scrollback + visible; `history_size = len - visible_lines` |

`compute_index(Line(l))` (`grid/storage.rs:220-234`):

```
positive = visible_lines - l - 1        // Line(visible_lines-1) → 0, Line(0) → visible_lines-1
zeroed   = zero + positive
index    = if zeroed >= inner.len() { zeroed - inner.len() } else { zeroed }
```

Negative lines (history) therefore map to indices *above* `visible_lines - 1`, and the whole thing
wraps modulo `inner.len()`.

| Operation | Behaviour | Cite |
|---|---|---|
| `with_capacity(visible, cols)` | allocates only the visible rows; scrollback grows lazily | `67-76` |
| `initialize(n, cols)` | if `len + n > inner.len()`, `rezero()` then grow `inner` by `max(n, MAX_CACHE_SIZE)`; then `len += n` | `126-138`; `MAX_CACHE_SIZE = 1000` (`:13`) |
| `shrink_lines(n)` | `len -= n` only; `inner` is freed lazily once it exceeds `len + 1000` | `107-114` |
| `truncate()` | `rezero()` then `inner.truncate(len)` — used before ref-test comparison and by `Grid::truncate` | `118-122` |
| `rotate(count: isize)` / `rotate_down(count)` | modular add on `zero` — O(1) scroll | `180-195` |
| `swap(a, b)` | unsafe 4-qword swap; `debug_assert_eq!(size_of::<Row<T>>(), 32)` | `153-176` |
| `PartialEq` | **asserts `zero == 0` on both sides**, then compares `inner` and `len` | `55-63` |

### 3.2 `Row<T>` and `occ`

`grid/row.rs:17-25`. `occ` is "the upper bound on cells modified since the last reset; everything
past it is guaranteed equal". It is maintained by the `IndexMut` impls: `Index<Column>` bumps it to
`index + 1`, `RangeFrom`/`RangeFull`/`IntoIterator<&mut>` set it to `len()`
(`grid/row.rs:202-276`).

`Row::reset(template)` (`grid/row.rs:91-110`): if the **last** cell's `ResetDiscriminant` (for
`Cell` that is `bg`, `term/cell.rs:113-117`) differs from the template's, set `occ = len` first;
then reset cells `0..occ`; then `occ = 0`. This is the BCE fast path.

`is_clear()` is `inner.iter().all(GridCell::is_empty)`. For `Cell`, `is_empty()` requires
`c ∈ {' ', '\t'}`, default fg/bg, none of `INVERSE | ALL_UNDERLINES | STRIKEOUT | WRAPLINE |
WIDE_CHAR_SPACER | LEADING_WIDE_CHAR_SPACER`, and an empty `zerowidth` list
(`term/cell.rs:250-265`). Note `WIDE_CHAR`, `BOLD`, `ITALIC`, `DIM`, `HIDDEN` and a hyperlink do
**not** make a cell non-empty.

`Row::shrink(columns)` returns the overflow with trailing empty cells trimmed;
`grow`, `append`, `append_front`, `front_split_off` maintain `occ` (`grid/row.rs:60-169`).

`Row::PartialEq` compares `inner` only — `occ` is **not** part of equality.

### 3.3 `Grid<T>`

`grid/mod.rs:110-138`. Fields: `cursor`, `saved_cursor` (both `#[serde(skip)]`), `raw: Storage<T>`,
`columns`, `lines`, `display_offset`, `max_scroll_limit`.

`Dimensions` (`grid/mod.rs:486-519`) derives everything from three methods:
`total_lines()` (= `raw.len()`), `screen_lines()`, `columns()`; from those,
`last_column() = columns - 1`, `topmost_line() = Line(-(history_size))`,
`bottommost_line() = Line(screen_lines - 1)`,
`history_size() = total_lines.saturating_sub(screen_lines)`.

| Op | Behaviour | Cite |
|---|---|---|
| `scroll_display(Scroll)` | `Delta(n)` → `clamp(offset + n, 0, history)`; `PageUp` → `+lines`; `PageDown` → `saturating_sub(lines)`; `Top` → `history_size()`; `Bottom` → 0 | `163-173` |
| `update_history(n)` | shrink the history if it exceeds `n`, clamp `display_offset`, set `max_scroll_limit = n` | `154-161` |
| `scroll_up(region, n)` | **if `region.end - region.start <= n` and `region.start != 0` → just reset the region and return** (no history). Otherwise: if `display_offset != 0` → `display_offset = min(offset + n, max_scroll_limit)`. If `region.start == 0` → `increase_scroll_limit(n)`, swap the fixed top rows, `raw.rotate(-n)`, swap the fixed bottom rows back. Else → plain swaps, no history. Finally reset the last `n` rows of the region | `252-307` |
| `scroll_down(region, n)` | if `region.end - region.start <= n` → reset the whole region and return. Two implementations depending on `max_scroll_limit == 0` (whole-buffer `rotate_down` vs. subregion swaps) — semantically identical: content moves down, top `n` rows of the region cleared, **nothing ever enters history** | `191-247` |
| `clear_viewport()` | scan backwards from the bottom-right for the last non-empty cell; `positions = that line + 1`; `scroll_up(full region, positions)` (→ history); reset rows `0..(lines - positions)`. Cursor is untouched | `309-333` |
| `clear_history()` | `raw.shrink_lines(history_size())`, `display_offset = 0` | `383-389` |
| `reset()` | `clear_history()`, default both cursors, `display_offset = 0`, reset every row from `topmost_line()` down | `336-352` |
| `reset_region(bounds)` | reset the given viewport line range with `cursor.template` (so BCE applies) | `357-380` |
| `display_iter()` | starts at `Point(Line(-display_offset - 1), last_column)`, ends at `Point(min(start.line + screen_lines, bottommost_line), last_column)`; because `next()` advances *before* yielding, the first cell is `(Line(-display_offset), Column(0))` | `422-429` |
| `iter_from(point)` | same pre-increment semantics, end is `(bottommost_line, last_column)` | `412-415` |
| `PartialEq` | compares `raw`, `columns`, `lines`, `display_offset` — **not** `max_scroll_limit`, cursors or `occ` | `443-451` |

`GridIterator` is bidirectional (`grid/mod.rs:592-656`); `prev()` stops at
`(topmost_line, Column(0))`, `next()` stops at `end`. `size_hint` is exact
(test `accurate_size_hint`, `grid/tests.rs:351-376`).

### 3.4 `Point`, `Line`, `Column`, `Boundary`

`index.rs`. `Line(i32)` is signed: `0..screen_lines` is the viewport, negatives are history.
`Point` ordering is line-major (`index.rs:123-129`).

| Helper | Rule | Cite |
|---|---|---|
| `Point::add(dims, boundary, n)` | `line += (n + column) / cols; column = (column + n) % cols`, then `grid_clamp` | `79-87` |
| `Point::sub(dims, boundary, n)` | `line -= (n + cols - 1).saturating_sub(column) / cols; column = (cols + column - n % cols) % cols`, then `grid_clamp` | `65-74` |
| `Boundary::Cursor` | clamp to `Line(0)..=bottommost_line`; underflow snaps to `(0, 0)`, overflow to `(bottommost, last_column)` | `102-114, 141-148` |
| `Boundary::Grid` | clamp to `topmost_line..=bottommost_line` | same |
| `Boundary::None` | wrap modulo `total_lines` (vi mode) | `149-162` |

---

## 4. Events

`event.rs:14-69`. `EventListener::send_event(&self, Event)` (`event.rs:117-119`) is the whole
interface; `VoidListener` is the null sink. The proxy is `&self`, so it must be interior-mutable /
`Sync` — `Term` holds it by value.

| Variant | Fired by |
|---|---|
| `MouseCursorDirty` | `Term::scroll_display`; set/unset of private modes 1000/1002/1003 |
| `Title(String)` / `ResetTitle` | `set_title` (OSC 0/2, `CSI 23 t`), `set_options` |
| `ClipboardStore(ClipboardType, String)` | OSC 52 store, subject to `Config::osc52` |
| `ClipboardLoad(ClipboardType, Arc<Fn(&str)->String>)` | OSC 52 `?` |
| `ColorRequest(usize, Arc<Fn(Rgb)->String>)` | OSC 4/10/11/12 with `?` |
| `PtyWrite(String)` | DA1, DA2, DSR 5/6, DECRPM (both forms), `CSI 18 t`, `CSI ? u` |
| `TextAreaSizeRequest(Arc<Fn(WindowSize)->String>)` | `CSI 14 t` |
| `CursorBlinkingChange` | `set_cursor_style`, private mode 12 set/unset, `reset_state`, `toggle_vi_mode` |
| `Wakeup` | `event_loop` after a PTY read, iff `sync_bytes_count() < processed` |
| `Bell` | `BEL` |
| `Exit` | `Term::exit()` |
| `ChildExit(ExitStatus)` | `event_loop` on `ChildEvent::Exited` |
| **`Osc { params: Vec<Vec<u8>>, bell_terminated: bool }`** | OneTerm patch 0002 — `Term::report_osc`, i.e. every OSC vte does not consume itself (OSC 1, 7, 9, 133, …) |
| **`ClearScreen`** | OneTerm patch 0002 — `clear_screen(All \| Saved)` and `reset_state` |

`ClipboardType` is `Clipboard` / `Selection`; OSC 52's selection byte maps `'c' → Clipboard`,
`'p' | 's' → Selection`, anything else → the request is dropped (`term/mod.rs:1722-1726`).

`WindowSize { num_lines, num_cols, cell_width, cell_height }` (`event.rs:103-109`) and
`OnResize::on_resize` (`event.rs:112-114`) are the PTY-side resize contract.

---

## 5. `FairMutex`

`sync.rs:11-48`. Two `parking_lot::Mutex`es: `data` and a token `next`.

- `lock()` takes `next` then `data` (and drops `next` at the end of the statement).
- `lease()` returns a guard on `next` alone — a reservation that blocks every *fair* `lock()`
  without touching the data.
- `lock_unfair()` / `try_lock_unfair()` skip `next` entirely.

Why alacritty needs it: the PTY reader thread would otherwise starve the UI. `event_loop::read_pty`
(`event_loop.rs:120-170`) takes `try_lock_unfair()`, falls back to a blocking `lock_unfair()` only
when `unprocessed >= READ_BUFFER_SIZE` (1 MiB), and **releases after `MAX_LOCKED_READ = 65535`
parsed bytes** so the renderer's fair `lock()` gets a turn. The lease is for the UI side: it can
reserve the next slot, do its own work, and be guaranteed the reader will not re-acquire ahead of
it.

A reimplementation needs the same three properties: a bounded hold time on the producer side, a way
for the consumer to reserve the next acquisition, and an unfair fast path for the producer.

---

## 6. Windows ConPTY

Files: `tty/windows/{mod.rs, conpty.rs, child.rs, blocking.rs}`. Split into *engine* vs *glue*:

**Engine-relevant contract** (must be re-implemented in some form):

- `tty::Options { shell: Option<Shell>, working_directory, drain_on_exit, env: HashMap, escape_args }`
  (`tty/mod.rs:22-47`).
- `EventedReadWrite` — `register`/`reregister`/`deregister` against a `polling::Poller`, plus
  `reader()` / `writer()` (`tty/mod.rs:68-82`).
- `EventedPty::next_child_event() -> Option<ChildEvent>` with `ChildEvent::Exited(Option<ExitStatus>)`
  (`tty/mod.rs:84-101`). The Windows impl maps a disconnected channel to `Exited(None)`
  (`tty/windows/mod.rs:113-119`).
- `OnResize::on_resize(WindowSize)`.
- `setup_env()` (`tty/mod.rs:102-113`): sets `TERM` to `alacritty` if that terminfo entry exists,
  else `xterm-256color`, and `COLORTERM=truecolor`. Uses `unsafe { env::set_var }` — process-global.
- Two poll tokens: `PTY_CHILD_EVENT_TOKEN = 1`, `PTY_READ_WRITE_TOKEN = 2`
  (`tty/windows/mod.rs:21-22`).

**Windows-API glue** (replaceable wholesale):

- `ConptyApi` (`conpty.rs:45-89`) prefers `conpty.dll` (Windows Terminal's OpenConsole) via
  `LoadLibraryW` + `GetProcAddress`, falling back to the in-box
  `CreatePseudoConsole`/`ResizePseudoConsole`/`ClosePseudoConsole`.
- `conpty::new` (`conpty.rs:110-244`): two anonymous pipes (`miow::pipe::anonymous(0)`),
  `CreatePseudoConsole(COORD{X: cols, Y: lines}, conin_pty, conout_pty, 0, &mut hpcon)`, then a
  `STARTUPINFOEXW` whose attribute list carries `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`, and
  `CreateProcessW` with `EXTENDED_STARTUPINFO_PRESENT` (plus `CREATE_UNICODE_ENVIRONMENT` when a
  custom env block is supplied). `STARTF_USESTDHANDLES` with null handles prevents handle
  inheritance.
- Default shell is `powershell` (`mod.rs:159`). `escape_args` selects between raw concatenation and
  MSVCRT-style quoting (`push_escaped_arg`, `mod.rs:130-156`; test table at `mod.rs:187-220`).
- Custom env: keys are deduplicated **case-insensitively**, user entries win, then the parent
  environment is appended; block is `name=value\0…\0\0` UTF-16 (`conpty.rs:250-301`).
- `Conpty::drop` calls `ClosePseudoConsole`, **which blocks until the conout pipe is drained** —
  hence the comment that `backend` must be the first field of `Pty` so it drops before `conout`
  (`mod.rs:28-34`, `conpty.rs:97-105`).
- `ChildExitWatcher` (`child.rs`): `RegisterWaitForSingleObject(..., WT_EXECUTEINWAITTHREAD |
  WT_EXECUTEONLYONCE)`; the callback reads `GetExitCodeProcess`, pushes `ChildEvent::Exited` down an
  `mpsc::Sender`, and posts an IOCP `CompletionPacket` carrying the registered `Event` so the poller
  wakes. `UnregisterWait` on drop.
- `blocking.rs`: `UnblockedReader`/`UnblockedWriter` run the blocking pipe I/O on named threads
  (`alacritty-tty-reader-thread`) feeding a `piper` ring buffer of `READ_BUFFER_SIZE`, and post IOCP
  completion packets to integrate with `polling`. `first_register` forces one spurious readiness
  event so the first poll doesn't hang.
- `on_resize` → `ResizePseudoConsole(handle, COORD{X: cols as i16, Y: lines as i16})`, and
  `assert_eq!(result, S_OK)` — a failed resize **panics** (`conpty.rs:303-308`).

---

## 7. Conformance corpus — alacritty ref tests

Location: `C:\Users\trunglt\.cargo\git\checkouts\alacritty-20195d12a03fa0c5\fcf32fe\alacritty_terminal\tests\`
(the same pinned revision the vendor tree is built from; `vendor/refresh.sh` prunes these from
`vendor/alacritty_terminal`). Licensed **Apache-2.0** (`alacritty_terminal/Cargo.toml:5`, repo
`LICENSE-APACHE`) — reusable with attribution and a `NOTICE`-style credit.

### 7.1 Layout

`tests/ref.rs` (138 lines) declares the tests via a macro (`ref.rs:17-76`); each name maps to
`tests/ref/<name>/` containing four files:

| File | Content |
|---|---|
| `alacritty.recording` | raw PTY output bytes, captured by the `--ref-test` flag (43 B – 351 KB) |
| `size.json` | `{"columns": N, "screen_lines": M}` → `term::test::TermSize` |
| `config.json` | `{"history_size": N}` → `Config::scrolling_history` |
| `grid.json` | the serialized expected `Grid<Cell>` (2.8 KB – 19 MB) |

### 7.2 Harness

`ref_test` (`ref.rs:100-137`):

```rust
let mut terminal = Term::new(Config { scrolling_history, ..Default::default() }, &size, Mock);
let mut parser = ansi::Processor::new();
parser.advance(&mut terminal, &recording);

let mut term_grid = terminal.grid().clone();
term_grid.initialize_all();   // fill history to max_scroll_limit + screen_lines
term_grid.truncate();         // rezero the ring buffer and drop the row cache
assert_eq!(grid, term_grid);
```

`Mock` is a no-op `EventListener`, so `PtyWrite` responses are discarded. The whole file is behind
`#![cfg(feature = "serde")]` (`ref.rs:1`), which gates the `Serialize`/`Deserialize` derives on
`Grid`, `Storage`, `Row`, `Cell`, `CellExtra`, `Flags`, `Color`, `Rgb`, `Point`, `Line`, `Column`,
`Hyperlink`, `GraphicId`/`GraphicCell` and `TermSize`.

### 7.3 What is actually compared

`grid.json` top level: `{ raw, columns, lines, display_offset, max_scroll_limit }`;
`raw` is `{ inner: [Row…], zero, visible_lines, len }`; each `Row` is `{ inner: [Cell…], occ }`;
each `Cell` is `{ c, fg, bg, flags, extra }` with `flags` as a bitflags string and `extra` either
`null` or `{ zerowidth, underline_color, hyperlink, graphic }`.

But the comparison is narrower than the file:

- `Grid::eq` → `raw` + `columns` + `lines` + `display_offset` (`grid/mod.rs:443-451`).
  **`max_scroll_limit` is not compared.**
- `Storage::eq` → asserts `zero == 0` on both, compares `inner` + `len`
  (`grid/storage.rs:55-63`). **`visible_lines` is not compared.**
- `Row::eq` → `inner` only (`grid/row.rs:27-31`). **`occ` is not compared.**
- `cursor` / `saved_cursor` are `#[serde(skip)]` (`grid/mod.rs:112-117`) — **cursor position, the
  SGR template, charsets and `input_needs_wrap` are never checked**, nor are `TermMode`, the colour
  overrides, the title, the tab stops or the scroll region.

So the corpus pins **cell content, per-cell attributes, wrap flags, row count and display offset**
after replaying a byte stream. That is exactly the surface a rewrite is most likely to break, and it
costs nothing to adopt: a new engine only needs a shim that produces the same JSON shape (or a
converter that reads these files into its own cell representation).

### 7.4 The 45 recordings

`alt_reset`, `clear_underline`, `colored_reset`, `colored_underline`, `csi_rep`, `decaln_reset`,
`deccolm_reset`, `delete_chars_reset`, `delete_lines`, `erase_chars_reset`, `erase_in_line`,
`fish_cc`, `grid_reset`, `history`, `hyperlinks`, `indexed_256_colors`, `insert_blank_reset`,
`issue_855`, `ll`, `newline_with_cursor_beyond_scroll_region`, `origin_goto`, `region_scroll_down`,
`row_reset`, `saved_cursor`, `saved_cursor_alt`, `scroll_in_region_up_preserves_history`,
`scroll_up_reset`, `selective_erasure`, `sgr`, `tab_rendering`, `tmux_git_log`, `tmux_htop`,
`underline`, `vim_24bitcolors_bce`, `vim_large_window_scroll`, `vim_simple_edit`,
`vttest_cursor_movement_1`, `vttest_insert`, `vttest_origin_mode_1`, `vttest_origin_mode_2`,
`vttest_scroll`, `vttest_tab_clear_set`, `wrapline_alt_toggle`, `zerowidth`, `zsh_tab_completion`.

Notable configurations: `history` (1000 lines of scrollback, 9.7 MB expected grid), `row_reset`
(1200 lines, 19 MB, 54 KB recording — the heaviest), `grid_reset` (100), `region_scroll_down` and
`scroll_in_region_up_preserves_history` (10), everything else `history_size: 0`.
Largest recordings: `vim_24bitcolors_bce` (351 KB), `vim_large_window_scroll` (303 KB),
`tmux_htop` (51 KB). Smallest: `selective_erasure` (43 B, a 10×3 grid — a good first target).

Upstream also ships unit tests worth porting directly: `vte/src/lib.rs` `mod tests`
(37 parser tests), `vte/src/ansi.rs` `mod tests` (23 tests incl. the four sync-update tests
`partial_sync_updates`, `sync_bursts_buffer`, `mixed_sync_escape`, `sync_bsu_with_esu`),
`grid/tests.rs` (10), `grid/storage.rs` `mod tests`, `selection.rs` `mod tests`,
`term/mod.rs` `mod tests` (23, incl. `damage_public_usage`, `damage_cursor_movements`,
`full_damage`, `window_title`).

---

## 8. Trap list

Concrete, testable behaviours a naive reimplementation gets wrong. Each is worth one test.

| # | Trap | Reference behaviour | Cite |
|---|---|---|---|
| 1 | **Pending wrap then `BS`** | Cursor at the last column with `input_needs_wrap` set: `BS` decrements the column *and clears the pending wrap*, so the next glyph lands at `columns-2`, not on a new line. `BS` at column 0 is a **no-op** — there is no reverse wrap even when the previous row has `WRAPLINE`. | `term/mod.rs:1410-1420` |
| 2 | **Pending wrap then `EL 0`** | `CSI K` returns immediately and erases **nothing** while `input_needs_wrap` is set. | `term/mod.rs:1659` |
| 3 | **Pending wrap then `HT`** | `put_tab` with a pending wrap performs `wrapline()` and returns — one tab consumes the whole wrap, no tab stop movement. | `term/mod.rs:1396-1399` |
| 4 | **Pending wrap with `LINE_WRAP` off** | `wrapline()` returns without clearing `input_needs_wrap`, so the flag stays set forever and every subsequent glyph overwrites the last column. | `term/mod.rs:972-974` |
| 5 | **Wide char at the last column** | With wrap on: a `' '` carrying `LEADING_WIDE_CHAR_SPACER` is written into the last column, then the line wraps and the glyph goes to columns 0–1 of the next row. With wrap **off**: `input_needs_wrap = true` and **the character is silently dropped**. | `term/mod.rs:1123-1137` |
| 6 | **Overwriting half a wide pair** | Writing a narrow char over a `WIDE_CHAR` must clear the `WIDE_CHAR_SPACER` to its right; writing over a `WIDE_CHAR_SPACER` must `clear_wide()` the cell to its left (dropping its zero-width list and setting `c = ' '`); and at column ≤ 1 the previous row's `LEADING_WIDE_CHAR_SPACER` must be cleared too. | `term/mod.rs:1007-1024` |
| 7 | **Insert mode over a wide char** | `input()` in `INSERT` mode shifts the row with raw `swap`s and never repairs wide pairs — the reference produces orphaned spacers. Also the shift is **skipped entirely** when `cursor.column + width >= columns`. | `term/mod.rs:1104-1113` |
| 8 | **Zero-width char at column 0** | With no pending wrap, `column.saturating_sub(1)` stays at 0, so the combining mark attaches to column 0 rather than to the previous row's last cell. | `term/mod.rs:1082-1096` |
| 9 | **`ED 2` on the primary screen keeps content** | `CSI 2 J` scrolls the occupied viewport into **scrollback** (`clear_viewport`) instead of discarding it, leaves `display_offset` unchanged, and does **not** move the cursor. On the alt screen it is a plain `reset_region(..)`. | `term/mod.rs:1793-1809`, `grid/mod.rs:309-333`; tests `clearing_viewport_keeps_history_position` |
| 10 | **`ED 3` resets `display_offset`** | `clear_history()` sets `display_offset = 0`, so a scrolled-back view snaps to the bottom. With **zero** history the grid is untouched but `Event::ClearScreen` still fires. | `term/mod.rs:1765, 1810-1837`; test `clearing_scrollback_resets_display_offset` |
| 11 | **`ED 1` skips line 0 when the cursor is on line 1** | The guard is `if cursor.line > 1`, not `> 0`. | `term/mod.rs:1770` |
| 12 | **`CSI 2 J` vs `Event::ClearScreen`** | Only `ED 2`, `ED 3` and `RIS` emit `ClearScreen`; `ED 0`/`ED 1`/`EL *` do not. The event fires **before** the `history_size() > 0` check for `ED 3`. | `term/mod.rs:1765, 1834-1837` (OneTerm patch 0002) |
| 13 | **`CSI ? 47` / `? 1047` / `? 1048` do nothing** | Only `1049` is a `NamedPrivateMode`; the others become `Unknown` and are logged and dropped, so the classic alt-screen sequences are silently ignored. | `ansi.rs:895-916`, `term/mod.rs:1944-1951` |
| 14 | **`? 1049 h` clobbers the primary DECSC slot** | Entering the alt screen sets `grid.saved_cursor = grid.cursor`, so a `DECSC` taken before entering is lost; leaving restores the cursor *as it was at entry*, not the pre-`DECSC` value. Each grid keeps its own `saved_cursor` otherwise. | `term/mod.rs:734-742` |
| 15 | **`DECSTBM` with an invalid range is a no-op** | `top >= bottom` leaves the previous region intact instead of resetting it. And every valid `DECSTBM` ends with `goto(0,0)`. | `term/mod.rs:2181-2201` |
| 16 | **Cursor outside the scroll region** | `goto` clamps to the region **only** in origin mode; `LF` below the region walks down to the screen bottom without scrolling; `IL`/`DL` outside the region are complete no-ops. | `term/mod.rs:1167-1184, 1434-1444, 1496-1515`; ref test `newline_with_cursor_beyond_scroll_region` |
| 17 | **Scrollback only fills from a region starting at line 0** | `Grid::scroll_up` pushes rows into history only when `region.start == Line(0)`; a `DECSTBM 5;20` region scrolling up discards its top line. Worse: if `region.end - region.start <= positions` **and** `region.start != 0`, the region is simply blanked with no rotation at all. | `grid/mod.rs:252-307`; ref test `scroll_in_region_up_preserves_history` |
| 18 | **`SD` / `CSI T` never touches history** | `scroll_down` always blanks the top of the region; it never pulls rows back out of scrollback. | `grid/mod.rs:191-247` |
| 19 | **`DCH` clamps `end` to `columns - 1`** | `end = min(start + count, columns - 1)` — for `count >= columns - start` this is *not* a plain "shift left by count"; the cell at `columns - 1` participates in the swap loop. Reproduce the arithmetic literally. | `term/mod.rs:1546-1572` |
| 20 | **`SGR 38;5` vs `38:5`** | `38;5;n` consumes exactly one following *parameter*; `38:5:n` reads a *subparameter*. `38:2:<cs>:R:G:B` (6 subparams) skips the colour-space id but `38:2:R:G:B` (5) does not. A value > 255 anywhere in the colour aborts the attribute after the params are already consumed. | `ansi.rs:1893-1961` |
| 21 | **`SGR 4:x` underline styles** | `4:0` cancels, `4:2/3/4/5` select double/curly/dotted/dashed, `4:1` and any other subparam fall back to plain `Underline`. `Attr::*Underline` always clears `ALL_UNDERLINES` before setting one bit. `SGR 21` is `CancelBold`, **not** double underline. | `ansi.rs:1860-1868`, `term/mod.rs:1908-1932` |
| 22 | **A parameter of `0` means "default"** | `next_param_or` treats `0` as absent, so `CSI 0 A` == `CSI A` == up 1, and `CSI 0 SP q` == `CSI SP q` == "reset cursor style". | `ansi.rs:1568-1571` |
| 23 | **CSI ignore vs CSI overflow** | A private marker appearing in `CsiParam` (`CSI 1 ? m`) routes to `CsiIgnore` and dispatches **nothing**; 33+ parameters still dispatch with `ignore = true`, and `ansi.rs` then drops the sequence. Two different paths, same visible result but different `Perform` traces. | `lib.rs:217-255, 453-460`; `ansi.rs:1563-1566` |
| 24 | **OSC ends only on BEL or `ESC \`** | C1 `ST` (`0x9C`) inside an OSC is payload, not a terminator. `ESC \` produces two `Perform` callbacks. With `std`, the OSC buffer is unbounded — impose your own limit but expect recordings that exceed 1 KB (OSC 52). | `lib.rs:407-435`; `CHANGELOG.md` 0.8.0 |
| 25 | **OSC 52 defaults to copy-only** | `Config::osc52` defaults to `Osc52::OnlyCopy`, so `OSC 52;c;?` (paste) is **denied**. The selection byte must be `c`, `p` or `s`; anything else drops the request. Base64 that fails to decode, or decodes to invalid UTF-8, is silently dropped. | `term/mod.rs:373-387, 1719-1762` |
| 26 | **OSC 4 needs an odd parameter count** | `OSC 4;1;#ff0000;2` (even) is rejected wholesale. Index parsing is `u8`-only: `OSC 4;300;…` fails. `OSC 104` with no argument resets indices 0–255 but **not** 256/257/258. | `ansi.rs:1372-1408, 1512-1532`, `224-236` |
| 27 | **Tab stops after a resize** | `TabStops::resize` keeps cleared stops in the retained prefix and adds *default* stops (absolute multiples of 8) in the grown region. Shrink-then-grow therefore resurrects stops that `TBC 3` had cleared. `CSI ? 5 W` (reset to every 8th) is parsed but **unimplemented** in `Term`. | `term/mod.rs:2430-2438`, `ansi.rs:615` |
| 28 | **Resize destroys the scroll region and (sometimes) the selection** | `Term::resize` unconditionally resets `scroll_region` to the full screen; it nulls the selection when the **column** count changed, and only rotates it when the column count is unchanged. | `term/mod.rs:697-713` |
| 29 | **The alt screen never reflows** | `grid.resize(!is_alt, …)` / `inactive_grid.resize(is_alt, …)`: reflow is enabled only for whichever grid is the *primary* one. With reflow off, `shrink_columns` truncates and merely clamps the cursor column. | `term/mod.rs:694-696`, `grid/resize.rs:374-387` |
| 30 | **Resize with `display_offset > 0`** | Both reflow paths adjust `display_offset` as rows are created/destroyed above the viewport (`+= 1` / `-= 1`) and finish with `min(display_offset, history_size())`; `grow_lines` additionally does `display_offset.saturating_sub(lines_added)`. Getting this wrong makes the view jump while the user is scrolled back. | `grid/resize.rs:67, 190-193, 241, 358-361, 372` |
| 31 | **Reflow re-arms the pending wrap** | Both column paths convert `input_needs_wrap` into `column += 1` (a column *equal to* `columns`) before reflowing and re-derive the flag afterwards; `shrink_columns` re-arms it only when the last cell lacks `WRAPLINE`. | `grid/resize.rs:114-117, 174-177, 249-252, 377-381` |
| 32 | **`shrink_columns` truncates history** | `reversed.truncate(max_scroll_limit + lines)` discards the oldest rows created by splitting. Narrowing a terminal can therefore *lose* scrollback that a naive implementation would keep. | `grid/resize.rs:368` |
| 33 | **Reflow inserts and removes `LEADING_WIDE_CHAR_SPACER`** | Growing drops a trailing leading-spacer from the join target; shrinking replaces a `WIDE_CHAR` that would land in the new last column with a leading spacer and moves the glyph to the next row, and moves `WRAPLINE` one cell left when the wrapped part ends in a leading spacer. | `grid/resize.rs:136-162, 289-312` |
| 34 | **`INSERT` mode forces full damage every frame** | `Term::damage()` calls `mark_fully_damaged()` whenever `TermMode::INSERT` is set — entering insert mode silently disables partial redraw. Leaving it damages fully once, via `unset_mode`. | `term/mod.rs:455-459, 2151-2158`; test `full_damage` |
| 35 | **Damage indices shift with `display_offset`** | `TermDamageIterator` truncates to `screen_lines - display_offset` entries and reports `line + display_offset`. Content changes while scrolled back are covered by *full* damage instead (`scroll_up` sets it). | `term/mod.rs:190-198, 481-485` |
| 36 | **`Row::reset` only clears up to `occ`** | Unless the row's last cell's background differs from the template's, cells past `occ` are left untouched (they are provably already default). If your row type has a different "guaranteed equal past occ" invariant, BCE fills will be wrong. The ref tests do **not** compare `occ`, so this is only observable through content. | `grid/row.rs:91-110` |
| 37 | **`Cell::is_empty` ignores `WIDE_CHAR`, `BOLD`, `DIM`, `ITALIC`, `HIDDEN` and hyperlinks** | A bold space is "empty" and will be discarded by `Row::shrink` and by `clear_viewport`'s scan; a `WIDE_CHAR_SPACER` is not. | `term/cell.rs:250-265` |
| 38 | **CPR ignores origin mode** | `CSI 6 n` answers with the absolute cursor position even when `DECOM` is set; DEC terminals report region-relative. | `term/mod.rs:1356-1360` |
| 39 | **`RIS` keeps vi mode and the colour palette** | `mode &= VI` then `|= default`; `self.colors` is never touched, so OSC 4/10/11/12 overrides survive. `DECSTR` (`CSI ! p`) is not implemented at all. | `term/mod.rs:1841-1875`; `ansi.rs:1786` |
| 40 | **`DECCOLM` does not change the column count** | `CSI ? 3 h` **and** `CSI ? 3 l` both run `deccolm()`: reset the scroll region to full, wipe the grid, full damage. Width is unchanged, and DECRQM reports `NotSupported` for mode 3. | `term/mod.rs:854-866, 2108` |
| 41 | **Sync updates only match the exact 8 bytes** | `\x1b[?2026h` / `\x1b[?2026l` byte-for-byte; `\x1b[?2026;1h` will not end an update. The scan runs **in reverse** over the newly added window, so a BSU after an ESU in the same chunk restarts the update. And nothing expires the timeout on its own — the host must poll `sync_timeout()` and call `stop_sync`. | `ansi.rs:388-416, 448-465` |
| 42 | **Kitty keyboard: report reads the stack, push overflows the wrong stack** | `CSI ? u` reports the top of `keyboard_mode_stack`, not the live `TermMode` bits (they can diverge after `CSI = Ps u`). The 4096-entry overflow guard pops from `title_stack` — reproduce or consciously fix, but decide deliberately. | `term/mod.rs:1290-1311` |
| 43 | **`CSI b` (REP) replays through `input()`** | The repeated character goes through the full print path, so it wraps, honours insert mode, and re-triggers wide-char handling. `preceding_char` is set by `Perform::print`, i.e. by the *last printed* char, and survives across intervening escape sequences. | `ansi.rs:1288-1291, 1576-1584` |
| 44 | **Grid equality ignores cursor, mode, colours and `max_scroll_limit`** | If you adopt the ref corpus, remember it validates cell content and geometry only — you still need your own tests for cursor position, `input_needs_wrap`, mode bits and palette state. | `grid/mod.rs:443-451, 112-117` |
| 45 | **`Storage::PartialEq` asserts `zero == 0`** | Comparing two grids without `truncate()`/`rezero()` first **panics**. Any equality-based test harness must rezero. | `grid/storage.rs:55-63` |
| 46 | **`display_iter` / `iter_from` advance before yielding** | `iter_from(p).next()` returns the cell *after* `p`, and `display_iter()` starts one cell before the top-left so its first item is `(Line(-display_offset), Column(0))`. Off-by-one here shifts the whole screen. | `grid/mod.rs:412-429, 592-610` |
| 47 | **`ESC \` in an OSC vs a DCS** | In `OscString`, `ESC` dispatches the OSC and enters `Escape` (the `\` then no-ops). In `DcsPassthrough`, `ESC` calls `unhook()` **and** `reset_params()`, so the next escape's intermediates are clean. In `SosPmApcString`/`DcsIgnore` everything is dropped until `ESC`/`CAN`/`SUB`. | `lib.rs:417-422, 328-332, 438-450` |
| 48 | **8-bit C1 is not an introducer** | `\x9b` does not start a CSI; it reaches `execute(0x9B)` and is logged as unhandled. Only `0x9C` inside `DcsPassthrough` is special-cased. | `lib.rs:713-721, 335-338` |

---

## 9. Minimum viable API surface for the replacement

For reference, the symbols OneTerm actually consumes from these crates (everything else can be
dropped):

- `vte::ansi::{Processor, Handler, Params, Rgb, NamedColor, Color, CursorShape, CursorStyle,
  Hyperlink, StandardCharset, CharsetIndex, Attr, KeyboardModes, …}` and `vte::Params`.
- `alacritty_terminal::{Term, Grid}`; `term::{Config, Osc52, TermMode, TermDamage,
  LineDamageBounds, RenderableContent, RenderableCursor, ClipboardType, cell::{Cell, Flags,
  Hyperlink}, color::Colors, graphics::{GraphicId, GraphicCell, GraphicData}, test::TermSize}`.
- `grid::{Dimensions, Scroll, GridIterator, Indexed}`; `index::{Line, Column, Point, Side,
  Boundary, Direction}`; `selection::{Selection, SelectionType, SelectionRange}`.
- `event::{Event, EventListener, WindowSize, OnResize, Notify}`; `sync::FairMutex`;
  `tty::{Options, Shell, setup_env, EventedPty, EventedReadWrite, ChildEvent}`; `event_loop`.
