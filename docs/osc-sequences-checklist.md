# OSC (Operating System Command) Sequences — Checklist & Support Matrix

> Tài liệu tham khảo về các OSC escape sequences (cả **common** lẫn **vendor-specific**),
> kèm checklist theo nhóm và bảng mức độ hỗ trợ cho các terminal:
> **OneTerm** (project này), **Windows Terminal**, **Zed**, **Ghostty**
> (và tham chiếu iTerm2, Kitty, Alacritty, xterm, VTE, VS Code).

---

## ⚠️ Methodology & mức độ chắc chắn

Ma trận hỗ trợ dưới đây được **verify thực tế** (không phải ước đoán từ kiến thức cũ).
Mỗi cột có nguồn khác nhau — độ tin cậy ghi rõ đây:

| Terminal | Nguồn verify | Độ chắc chắn | Ngày test |
|----------|--------------|:------------:|----------|
| **OneTerm** | Codebase (`crates/core/src/terminal/osc.rs`, `osc_color.rs`, `listener.rs`, `shell.rs`) | 🟢 Rất cao | code hiện tại |
| **Windows Terminal** | MS Learn docs + GitHub PRs (#15727, #18449, #5823, color-query PR) + ansicode.eversources.app | 🟢 Cao | docs + PRs 2023–2025 |
| **Zed** | zed.dev/docs/terminal + source `terminal_hyperlinks.rs` + issue #17848 | 🟢 Cao | 2025–2026 |
| **Ghostty** | terminfo.dev (test thực, v1.3.1) + `src/terminal/osc.zig` | 🟢 Rất cao | test 2026-06-18 |
| **iTerm2** | terminfo.dev (test thực, v3.6.9) | 🟢 Rất cao | test 2026-06-18 |
| **Kitty** | terminfo.dev (test thực, v0.46.2) | 🟢 Rất cao | test 2026-06-18 |
| **Alacritty** | `docs/escape_support.md` (v0.13.2) official + PR #5769 + config docs | 🟢 Rất cao | v0.13.2 |
| **VS Code** | terminfo.dev (test thực, xterm.js) | 🟢 Rất cao | test 2026-06-18 |
| **xterm** | prior knowledge (xterm là *nguồn gốc* của nhiều OSC; ctlseqs doc) | 🟡 Trung bình | — |
| **VTE** (gnome-terminal) | prior knowledge | 🟡 Trung bình | — |

> 🟡 = chưa web-verify từng ô, dựa trên hiểu biết chung. Các cột 🟢 đã verify bằng test thực/docs/source.
> **Quan trọng**: nhiều giá trị trong phiên bản cũ của tài liệu này **SAI** — đã sửa dựa trên verify
> (VD: Alacritty **có** OSC 52/4/10-12/8 nhưng **không** OSC 7/133; Ghostty **không** OSC 17/19 set;
> iTerm2/Kitty/Ghostty/VS Code **đều có** OSC 633; VS Code **có** OSC 1337 image).

---

## 0. Cơ bản về OSC

### 0.1 Cú pháp chung

```
ESC ] Ps ; Pt ST
```

- `ESC ]` = `\x1b]` — mở đầu OSC.
- `Ps` — command number (có thể có nhiều tham số cách nhau bởi `;`).
- `Pt` — payload (text/color spec/URI/...).
- `ST` (String Terminator) — kết thúc OSC, một trong hai dạng:
  - `BEL` = `\x07` (phổ biến nhất, xterm de-facto).
  - `ESC \` = `\x1b\\` (chuẩn ECMA-48).

> ⚠️ Ghostty/iTerm2 cố gắng echo lại đúng terminator mà request dùng, để tối đa tương thích.
> Khi viết thư viện, ưu tiên `BEL` cho tương thích tối đa. OSC 8 theo spec nên dùng `ESC \`.

### 0.2 Query mode

Nhiều OSC hỗ trợ **query**: gửi `Pt = ?` để yêu cầu terminal báo lại giá trị hiện tại.
Ví dụ: `ESC ] 10 ; ? BEL` → hỏi màu foreground mặc định.
**Lưu ý**: không phải terminal nào cũng trả lời query (VD: Alacritty không có OSC 7/133;
Ghostty/Kitty **không** phản hồi OSC 52 read — chỉ write).

### 0.3 Color spec format

- `rgb:RRRR/GGGG/BBBB` — 16-bit/channel (đầy đủ, khuyến nghị).
- `rgb:RR/GG/BB` — 8-bit/channel.
- `#RRGGBB` — hex (hầu hết terminal chấp nhận).
- `?` — query giá trị hiện tại.

---

## Nhóm A — Window / Icon / Title (Cửa sổ & tiêu đề)

| Check | OSC | Mục đích | Format | Ghi chú |
|:-----:|-----|----------|--------|---------|
| ☐ | **0** | Đặt **cả** icon name + window title | `ESC]0;title ST` | Phổ biến nhất, dùng cho tab title. |
| ☐ | **1** | Đặt **icon name** (không đổi title) | `ESC]1;name ST` | Di sản X11. Alacritty **REJECTED**. |
| ☐ | **2** | Đặt **window title** | `ESC]2;title ST` | Tương đương OSC 0 cho hầu hết terminal hiện đại. |

### A.1 Mức độ hỗ trợ

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 0 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 1 | ❌ | ◐ | ◐ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ |
| 2 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

---

## Nhóm B — Color Palette (indexed colors 0–255)

| Check | OSC | Mục đích | Format | Ghi chú |
|:-----:|-----|----------|--------|---------|
| ☑ | **4** | Đặt/truy vấn 1+ màu palette | `ESC]4;idx:spec ST` | Query: `idx:?`. ✅ OneTerm. |
| ☐ | **5** | Đặt/truy vấn màu "đặc biệt" | `ESC]5;idx:spec ST` | iTerm2/VS Code/Alacritty **không** hỗ trợ. |
| ☑ | **104** | Reset 1+ màu palette | `ESC]104;idx ST` hoặc `ESC]104 ST` (all) | xterm origin. ✅ OneTerm. |
| ☐ | **105** | Reset màu đặc biệt | `ESC]105;idx ST` | Hiếm. |

### B.1 Mức độ hỗ trợ

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 4   | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 5   | ❌ | ◐ | ❌ | ◐ | ❌ | ✅ | ❌ | ✅ | ◐ | ❌ |
| 104 | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 105 | ❌ | ◐ | ❌ | ◐ | ◐ | ◐ | ❌ | ✅ | ◐ | ◐ |

> ✅ **OneTerm** (từ bản mới): OSC 4 **set + query (`idx;?`)** và OSC 104 **reset** (single/all) đã hỗ trợ.
> Dùng chung hạ tầng `ColorRequest` với OSC 10/11/12: set → `Term.colors[0..256]`, query → reply sau
> parse batch (fallback default palette qua `default_color_for_index` + `set_default_colors`), render qua
> `dynamic_colors().indexed` + `TerminalPalette.indexed`. OSC 5/105 (special colors) vẫn ❌.

---

## Nhóm C — Default & Special Colors (fg/bg/cursor/selection)

| Check | OSC | Mục đích | Query | Reset OSC | Ghi chú |
|:-----:|-----|----------|:-----:|:---------:|---------|
| ☑ | **10** | Foreground mặc định | `10;?` | **110** | Phổ biến. ✅ OneTerm. |
| ☑ | **11** | Background mặc định | `11;?` | **111** | Phổ biến. ✅ OneTerm. |
| ☑ | **12** | Text cursor color | `12;?` | **112** | ✅ OneTerm. |
| ☐ | **13** | Mouse pointer fg color | `13;?` | **113** | Hiếm; có reset 113 ở nhiều terminal. |
| ☐ | **14** | Mouse pointer bg color | `14;?` | **114** | Hiếm; có reset 114. |
| ☐ | **17** | Selection (highlight) bg | `17;?` | **117** | Kitty/iTerm2/VS Code có reset 117. |
| ☐ | **19** | Selection (highlight) fg | `19;?` | **119** | Kitty/iTerm2/VS Code có reset 119. |
| ☑ | **110–112** | Reset fg/bg/cursor | — | — | ✅ OneTerm. |
| ☐ | **117/119** | Reset selection bg/fg | — | — | |
| ☐ | **39** | Default fg (xterm alias OSC 10) | — | — | Ít phổ biến. |

### C.1 Mức độ hỗ trợ

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 10/11 | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 12 (cursor) | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 13–14 (pointer) | ❌ | ❌ | ❌ | ◐ | ◐ | ✅ | ❌ | ✅ | ❌ | ◐ |
| 17/19 (selection set) | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ | ❌ | ✅ | ❌ | ❌ |
| 110–112 (reset) | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 117/119 (reset sel) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ✅ | ❌ | ✅ |

> ✅ **OneTerm** (từ bản mới): OSC 10/11/12 **set + query (`?`)** và OSC 110/111/112 **reset** đã hỗ trợ.
> `Event::ColorRequest` được enqueue trong `LocalListener`/`SshListener` rồi trả lời sau mỗi parse batch
> (đọc `Term.colors()`, fallback theme default qua `set_default_colors`); set/reset render qua `dynamic_colors()`.
> ⚠️ Ghostty có **reset** 117/119 nhưng **không** hỗ trợ **set** 17/19 (terminfo: "No OSC 17/19 response").
> Nhiều terminal có reset 113/114 (pointer) mà không явно list set 13/14 → đánh ◐.

---

## Nhóm D — Clipboard

| Check | OSC | Mục đích | Format | Ghi chú |
|:-----:|-----|----------|--------|---------|
| ☑ | **52** | Đặt/query clipboard (base64) | `ESC]52;c;base64 ST` | `c`=clipboard, `p`=primary. Query: `c?`. ✅ OneTerm (write+read). |

### D.1 Lưu ý bảo mật & mức hỗ trợ

OSC 52 gây tranh cãi bảo mật (đọc clipboard). Nhiều terminal **chỉ write, không read** hoặc cần config.

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 52 | ✅ | ✅ | ❌ | ◐ | ✅ | ◐ | ✅ | ✅ | ✅ | ✅ |

- **OneTerm**: ✅ write (luôn bật), ◐ read (mặc định **tắt**). `OscSink` parse base64 (set) + query `?`;
  set đi qua alacritty `ClipboardStore` → `SessionEvent::Clipboard`; read (`52;c;?`) →
  `SessionEvent::ClipboardRead` → UI trả lời `52;c;<base64>` (`encode_osc52`) **chỉ khi** setting
  `security.allow_clipboard_read = true` (mặc định `false`, vì read để lộ clipboard local cho chương trình,
  kể cả remote qua SSH).
- **Windows Terminal**: ✅ — merged (PR #18449/#5823); có setting disable.
- **Zed**: ❌ — vẫn là feature request mở (issue #17848), chưa implement.
- **Ghostty**: ◐ — **write ✅, read ❌** (terminfo: "No OSC 52 read response").
- **iTerm2**: ✅ — read + write đều OK (cần enable "Allow clipboard access").
- **Kitty**: ◐ — **write ✅, read ❌** (terminfo).
- **Alacritty**: ✅ — config `terminal.osc52 = "OnlyCopy"|"OnlyPaste"|"CopyPaste"|"Disabled"`.
- **VS Code**: ✅ — read + write (terminfo).

---

## Nhóm E — Hyperlinks (OSC 8)

| Check | OSC | Mục đích | Format | Ghi chú |
|:-----:|-----|----------|--------|---------|
| ☐ | **8** | Mở/kết thúc hyperlink | `ESC]8;params;URL ST text ESC]8;; ST` | `id=ID` param để nhóm ô link. |

```
ESC ] 8 ; params ; URL ST   ← mở link
  <text hiển thị>
ESC ] 8 ; ; ST               ← đóng link
```

### E.1 Mức độ hỗ trợ

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 8 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |

- **OneTerm**: ✅ — alacritty VTE lưu hyperlink vào cell; `url.rs` detect (`link_ranges`/`url_at`).
- **Zed**: ✅ — `terminal_hyperlinks.rs` đọc `cell.hyperlink()` + `try_osc8_url_to_path`.
- **Alacritty**: ✅ — commit "Fixes #922" thêm OSC 8 (trang ansicode cũ đã sai khi ghi alacritty ❌).
- **xterm**: ❌ — không hỗ trợ OSC 8.

---

## Nhóm F — Current Working Directory (CWD)

| Check | OSC | Mục đích | Format | Ghi chú |
|:-----:|-----|----------|--------|---------|
| ☐ | **7** | Set CWD (file:// URI) | `ESC]7;file://host/path ST` | Chuẩn de-facto (VTE origin). |
| ☐ | **9;9** | Set CWD (ConEmu/Windows path) | `ESC]9;9;C:\path ST` | ConEmu/Windows Terminal. |

### F.1 Mức độ hỗ trợ

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 7 (file URI) | ✅ | ✅ | ◐ | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ✅ |
| 9;9 (ConEmu) | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

- **OneTerm**: ✅ OSC 7 — `OscSink` parse `file://` → `parse_cwd_url`. **Không** 9;9.
- **Alacritty**: ❌ OSC 7 — `escape_support.md` **không** list OSC 7 → alacritty_terminal **drop** nó.
  (Đó là lý do OneTerm phải tự parse OSC 7 qua `OscSink` song song với VTE.)
- **Windows Terminal**: ✅ cả 7 và 9;9 (MS docs).
- **Zed**: ◐ — dùng alacritty_terminal (drop OSC 7); có thể có parser riêng (không confirm trong docs).

---

## Nhóm G — Notifications & Progress

| Check | OSC | Mục đích | Format | Ghi chú |
|:-----:|-----|----------|--------|---------|
| ☑ | **9** | Desktop notification (iTerm2/WT) | `ESC]9;msg ST` | iTerm2 origin. ✅ OneTerm. |
| ☑ | **9;4** | Progress bar (ConEmu/WT) | `ESC]9;4;state;pct ST` | state: 0/1/2/3/4. WT 1.18+. ✅ OneTerm. |
| ☐ | **9;1/2/3** | ConEmu misc (sleep/msgbox/tabtitle) | `ESC]9;1;ms ST` v.v. | ConEmu-specific. |
| ☐ | **99** | Kitty notification (extended) | `ESC]99;i=ID;payload ST` | icon/focus/urgency. |
| ☐ | **777** | urxvt notification | `ESC]777;notify;title;body ST` | urxvt origin. |

### G.1 Mức độ hỗ trợ

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 9 (notif) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ◐ | ✅ |
| 9;4 (progress) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ◐ | ✅ |
| 9;1/2/3 (ConEmu) | ❌ | ◐ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 99 (kitty) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 777 (urxvt) | ❌ | ◐ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ◐ | ✅ |

> ✅ **OneTerm** (từ bản mới): OSC 9 (notification → toast qua `window.push_notification`) và OSC 9;4
> (progress → thanh progress mỏng ở mép trên terminal, state 0-4). Parse song song trong `OscSink`
> (alacritty drop OSC 9) → `OscPayload::Notification`/`Progress` → `SessionEvent`. Còn ❌: 9;1/2/3
> (ConEmu misc), 99 (kitty), 777 (urxvt).
> - **Ghostty**: ✅ tất cả (osc.zig có `conemu_*` cho 9;1–9;11 + `show_desktop_notification` cho 9/777/99).
> - **VS Code**: ✅ 9/9;4/99/777 (terminfo); 9;1/2/3 ❌.
> - **Alacritty/Zed**: ❌ toàn bộ notification.

---

## Nhóm H — Shell Integration / Prompt Markers

| Check | OSC | Mục đích | Format | Ghi chú |
|:-----:|-----|----------|--------|---------|
| ☐ | **133** | FinalTerm prompt markers | `133;A`/`B`/`C`/`D;exit` | Chuẩn de-facto shell integration. |
| ☐ | **133;P** | Prompt properties (kext) | `133;P;k=i ST` | Kitty/Ghostty/iTerm2/VS Code. |
| ☐ | **633** | VS Code shell integration | `633;A`..`D;exit`/`E`/`P` | VS Code own; nhiều terminal adopt. |
| ☐ | **633;SetMark** | VS Code mark | `633;SetMark ST` | Bookmark trong scrollback. |

### Chuẩn OSC 133 — 4 marker:

```
ESC]133;A ST      ← Prompt start
ESC]133;B ST      ← Command start
ESC]133;C ST      ← Command output start
ESC]133;D;exit ST ← Block end (exit code tuỳ chọn)
```

### H.1 Mức độ hỗ trợ

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 133 (A/B/C/D) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ✅ |
| 133;P | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 633 | ❌ | ✅ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 633;SetMark | ❌ | ◐ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |

- **OneTerm**: ✅ OSC 133 A/B/C/D (code: `Osc133Kind` enum + exit code). **Không** 133;P/633.
- **Alacritty**: ❌ OSC 133 — `escape_support.md` **không** list 133 → alacritty_terminal **drop** nó
  (lý do OneTerm phải tự parse qua `OscSink`).
- **iTerm2/Kitty/Ghostty/VS Code**: ✅ cả 133 + 633 + 133;P (terminfo).
- **Windows Terminal**: ✅ 133 + 633 (PR #15727 alias); 133;P ❌.
- **Zed**: ✅ 133 (discussion #44359); ❌ 633 (Zed dùng 133, 633 là VS Code-specific).

---

## Nhóm I — Font

| Check | OSC | Mục đích | Format | Ghi chú |
|:-----:|-----|----------|--------|---------|
| ☐ | **50** | Đặt/truy vấn font | `ESC]50;font-spec ST` | xterm origin. Alacritty chỉ CursorShape. |

### I.1 Mức độ hỗ trợ

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 50 | ❌ | ❌ | ❌ | ❌ | ◐ | ❌ | ◐ | ✅ | ❌ | ❌ |

- **Alacritty**: ◐ — OSC 50 IMPLEMENTED nhưng **chỉ CursorShape**, không font.
- **Kitty/iTerm2**: dùng OSC 710/7770/7777 (font riêng) thay vì 50.

---

## Nhóm J — Vendor-specific & Misc

| Check | OSC | Terminal/Context | Mục đích | Ghi chú |
|:-----:|-----|------------------|----------|---------|
| ☐ | **1337** | iTerm2 | Inline image + subcodes | `ESC]1337;File=...;inline=1:base64 ST`. |
| ☐ | **20** | (kext) | Background opacity | `ESC]20;alpha ST`. |
| ☐ | **46** | xterm | Log file | `ESC]46;path ST`. |
| ☐ | **21** | Kitty | Kitty color protocol | `ESC]21;... ST`. |
| ☐ | **22** | Kitty/Ghostty | Mouse pointer shape | `ESC]22;name ST`. |
| ☐ | **66** | Kitty | Text sizing | `ESC]66;... ST`. |
| ☐ | **3008** | systemd | Context signal (UAPI) | `ESC]3008;... ST`. |

### J.1 Mức độ hỗ trợ (chọn lọc)

| OSC | OneTerm | Win Terminal | Zed | Ghostty | iTerm2 | Kitty | Alacritty | xterm | VTE | VS Code |
|:----:|:-------:|:------------:|:---:|:-------:|:------:|:-----:|:---------:|:-----:|:---:|:-------:|
| 1337 (image) | ❌ | ❌ | ◐ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| 21 (kitty color) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 22 (mouse shape) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ |
| 20 (opacity) | ❌ | ❌ | ❌ | ❌ | ◐ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 46 (logfile) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ |

- **Inline image**: cạnh tranh giữa iTerm2 (OSC 1337), Kitty graphics (APC), Sixel.
  Ghostty/VS Code/iTerm2 ✅ OSC 1337; Kitty ❌ OSC 1337 (dùng Kitty graphics APC riêng);
  Windows Terminal ❌ (dùng Sixel từ 1.22).
- **OneTerm**: ❌ toàn bộ vendor-specific.

---

## Bảng tổng hợp nhanh — Top OSC thường dùng

> Checklist "must-have" khi target đa terminal. Cột OneTerm để đối chiếu project.

| Check | OSC | Tên | Độ phổ biến |
|:-----:|:---:|-----|:-----------:|
| ☐ | 0/2 | Window title | ⭐⭐⭐⭐⭐ |
| ☐ | 7 | CWD (file://) | ⭐⭐⭐⭐⭐ |
| ☐ | 8 | Hyperlinks | ⭐⭐⭐⭐ |
| ☑ | 4 | Color palette set/query | ⭐⭐⭐⭐ |
| ☑ | 10/11/12 | Default FG/BG/cursor | ⭐⭐⭐⭐ |
| ☑ | 52 | Clipboard | ⭐⭐⭐ |
| ☐ | 133 | Shell integration markers | ⭐⭐⭐⭐ |
| ☑ | 9 | Desktop notification | ⭐⭐⭐ |
| ☑ | 104/110-112 | Reset colors | ⭐⭐⭐ |
| ☐ | 633 | VS Code shell integration | ⭐⭐⭐ |

### OneTerm — tóm tắt tình trạng hiện tại

| Nhóm OSC | OneTerm | Đánh giá |
|----------|:-------:|----------|
| 0/2 title | ✅ | OK |
| 7 CWD | ✅ | OK (tự parse vì alacritty drop) |
| 8 hyperlink | ✅ | OK (qua alacritty cell) |
| 52 clipboard | ✅ | OK (tự parse + alacritty EventListener) |
| 133 shell integration | ✅ | OK (A/B/C/D + exit code) |
| 10/11/12 + 110–112 colors | ✅ | OK (set + query + reset fg/bg/cursor) |
| 4 + 104 palette colors | ✅ | OK (set + query + reset index 0–255) |
| 5/13–19/105/117–119 colors | ❌ | **Gap** — special/pointer/selection chưa map |
| 9 + 9;4 notification/progress | ✅ | OK (toast + progress bar) |
| 99/777 notifications | ❌ | **Gap** (kitty/urxvt) |
| 633 VS Code | ❌ | **Gap** (chỉ 133) |
| 1337 image | ❌ | **Gap** |

> OneTerm hiện **đủ** 5 nhóm cốt lõi (title/CWD/hyperlink/clipboard/shell-integration) **+ default colors
> (OSC 10/11/12/110-112) + color palette (OSC 4/104) + notification/progress (OSC 9, 9;4)**, nhưng **thiếu**
> special colors (5), pointer/selection (13–19), kitty/urxvt notification (99/777), VS Code 633, inline image.

---

## Legend

| Ký hiệu | Ý nghĩa |
|:-------:|---------|
| ✅ | Hỗ trợ đầy đủ (verify thực tế). |
| ◐ | Hỗ trợ một phần: chỉ subset tham số, chỉ write/read, cần config, hoặc chỉ reset không set. |
| ❌ | Không hỗ trợ (verify thực tế hoặc docs chính thức ghi REJECTED/missing). |
| 🟢/🟡 | Độ chắc chắn của nguồn cột (xem bảng Methodology). |
| ⭐ | Mức phổ biến (1–5, đánh giá chủ quan). |

---

## Kinh nghiệm thực tiễn

1. **ST terminator**: Dùng `BEL` (`\x07`) cho tương thích tối đa. OSC 8 theo spec nên dùng `ESC \`.
2. **Query response**: không phải terminal nào cũng trả lời query. Ghostty/Kitty **không** read OSC 52;
   Alacritty không có OSC 7/133; Ghostty không phản hồi OSC 5/17/19 query.
3. **OSC 52 clipboard**: luôn xử lý bị từ chối. Phân biệt **write** (phổ biến) vs **read** (hiếm, Ghostty/Kitty ❌).
4. **OSC 7 CWD**: phải là `file://` URI đầy đủ (gồm host). Alacritty upstream **không** hỗ trợ OSC 7
   → app dùng alacritty_terminal (như OneTerm/Zed) phải **tự parse** song song.
5. **Shell integration**: 133 (FinalTerm) là chuẩn chung; 633 là VS Code-specific nhưng iTerm2/Kitty/Ghostty
   cũng adopt. Phải bọc đúng 4 marker A/B/C/D.
6. **Color spec**: ưu tiên `rgb:RR/GG/BB` hoặc `rgb:RRRR/GGGG/BBBB`. Tránh `#hex` nếu cần tương thích xterm cũ.
7. **Vendor-specific**: chỉ dùng khi biết chắc terminal đích. Phát hiện qua `TERM`, `TERM_PROGRAM`,
   `WT_SESSION`, `KITTY_WINDOW_ID`, `GHOSTTY_RESOURCES_DIR`...
8. **Không lồng OSC**: đóng OSC trước khi mở OSC khác.
9. **Windows Terminal**: cross-OSC tốt (133+633+9;9+9;4+52+4/10/11/12). Dùng Sixel (1.22+) cho image, không Kitty graphics.
10. **Ghostty**: chuẩn hoá cao + mở rộng (133;P, 633, 9;1–11, 21, 22, 66, 3008, iTerm2 1337 image).
    **Không** Sixel, **không** OSC 17/19 set, **không** OSC 52 read.
11. **Zed**: 133 + 8 + 7 (qua alacritty VTE + parser riêng). **Không** OSC 52 (feature request mở),
    **không** 633, **không** notification. Dùng alacritty_terminal nên kế thừa điểm mạnh/yếu của nó.
12. **Alacritty**: có OSC 4/8/10/11/12/52/104/110-112 (config `terminal.osc52`).
    **Không** OSC 7/133/9/633/777. OSC 50 chỉ CursorShape. Cố ý tối giản.
13. **Kitty**: hỗ trợ rất rộng (4/5/7/8/10-19/21/22/52-write/66/99/104/110-119/133+P/633/777/3008...).
    **Không** OSC 1337 image (dùng Kitty graphics APC), **không** OSC 52 read, **không** Sixel.
14. **iTerm2**: near-complete (4/7/8/9/9;4/10-19/21/22/52/99/104/110-119/133+P/633/777/1337/3008...).
    **Không** OSC 5, **không** Sixel render (DA1 advertises nhưng không render).
15. **VS Code (xterm.js)**: hỗ trợ rộng bất ngờ (4/7/8/9/9;4/10-12/52/99/104/110-119/133+P/633/777/1337/3008...).
    **Không** OSC 5/17/19, **không** Kitty graphics display, **không** Sixel.
16. **OneTerm** (VTE = `alacritty_terminal`): Hỗ trợ **OSC 0/2, 7, 8, 52 (base64+query), 133 (A/B/C/D+exit),
    4 (set+query) + 104 (reset), 10/11/12 (set+query) + 110/111/112 (reset), 9 (notification), 9;4 (progress)**.
    - 133/9/9;4 được parse song song qua `OscSink` (alacritty VTE drop OSC 7/9/133); `OscSink` dùng queue
      FIFO nên nhiều OSC trong cùng một batch đọc đều được giữ + xử lý theo thứ tự.
    - OSC 8 lưu vào cell; OSC 52 đi qua `EventListener` + OscSink.
    - OSC 4/104 + 10/11/12/110-112: alacritty parse sẵn (set → `Term.colors`, reset → clear); OneTerm render
      qua `dynamic_colors()` (`TerminalPalette.indexed` cho index 0-255) và trả lời query qua
      `Event::ColorRequest` (enqueue → reply sau parse batch, fallback default palette qua `set_default_colors`
      + `default_color_for_index`).
    - OSC 9 → `SessionEvent::Notification` → toast `window.push_notification`; OSC 9;4 →
      `SessionEvent::Progress(TerminalProgress)` → thanh progress mỏng ở mép trên terminal view.
    - **Không** special colors (5/105), pointer/selection (13–19/113–119): chưa map;
    - **Không** notification 99 (kitty) / 777 (urxvt) / 9;1-3 (ConEmu misc), font (50), 633, 1337.
    - Tự sinh OSC 7 + 133 A qua `PROMPT_COMMAND` (bash) / `PS1` (zsh) / `PROMPT` (cmd).

---

## Tham khảo (đã verify)

### Test thực tế (terminfo.dev — test matrix, June 2026)
- Ghostty — <https://terminfo.dev/terminals/ghostty> (v1.3.1, 231/254)
- iTerm2 — <https://terminfo.dev/terminals/iterm2> (v3.6.9, 238/254)
- Kitty — <https://terminfo.dev/terminals/kitty> (v0.46.2, 218/254)
- VS Code — <https://terminfo.dev/terminals/vs-code> (xterm.js, 223/254)
- OSC family — <https://terminfo.dev/osc>, <https://ansicode.eversources.app/en/family/osc>

### Docs / source chính thức
- Alacritty escape support — <https://github.com/alacritty/alacritty/blob/master/docs/escape_support.md> (v0.13.2)
- Alacritty OSC 52 config — <https://alacritty.org/config-alacritty.html> (`terminal.osc52`)
- Alacritty OSC 4 query PR — <https://github.com/alacritty/alacritty/pull/5769>
- Ghostty OSC source — <https://github.com/ghostty-org/ghostty/blob/main/src/terminal/osc.zig>
- Ghostty OSC 52 docs — <https://ghostty.org/docs/vt/osc/52>
- Windows Terminal shell integration — <https://learn.microsoft.com/en-us/windows/terminal/tutorials/shell-integration>
- Windows Terminal OSC 633 PR — <https://github.com/microsoft/terminal/pull/15727>
- Windows Terminal OSC 52 PRs — <https://github.com/microsoft/terminal/pull/5823>, <https://github.com/microsoft/terminal/pull/18449>
- VS Code shell integration — <https://code.visualstudio.com/docs/terminal/shell-integration>
- Zed terminal docs — <https://zed.dev/docs/terminal>
- Zed OSC 52 request — <https://github.com/zed-industries/zed/issues/17848>
- Zed hyperlinks source — `crates/terminal/src/terminal_hyperlinks.rs`
- xterm ctlseqs — <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html>
- FinalTerm OSC 133 spec — <https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md>

### Codebase OneTerm (verify nội bộ)
- `crates/core/src/terminal/osc.rs` — `OscSink`, `OscPayload`, `Osc133Kind`, `parse_cwd_url`, `decode_osc52`/`encode_osc52`
- `crates/core/src/terminal/osc_color.rs` — `DynamicColors`, `PendingColorQuery`, `default_color_for_index` (OSC 10/11/12/110-112)
- `crates/local/src/listener.rs` & `crates/ssh/src/listener.rs` — `ColorRequest` enqueue → reply sau parse batch (event_loop/task)
- `crates/core/src/config/shell.rs` — `resolve_shell` sinh OSC 7/133 theo shell kind
- `crates/core/src/terminal/url.rs` — OSC 8 hyperlink detect