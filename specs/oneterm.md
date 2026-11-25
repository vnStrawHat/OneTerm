# OneTerm

OneTerm is a terminal app for Windows: local shells, SSH and SFTP, in tabs and spaces side by
side. It is a set of crates ("The repository"); the app is the package `oneterm-app`, its
library and its binary `oneterm`. It is the user's terminal app of that name built again on
Quadra (github.com/vnStrawHat/Quadra), a native GPU-rendered UI framework: its versions go on
from 0.7.0, and **OneTerm 0.6** in this spec is every version before, built on another
toolkit, whose updater starts `oneterm.exe` (its source is `.archived/` of this
repository). The work started on 2026-11-15 inside Quadra's repository, where the app was
named 1Term until 2026-11-24 (the user's decision that day gave it the name of what it
replaces); the folders it kept under that name are taken along ("Taking over from OneTerm
0.6").

Its look is the user's design (the canvas "1Term", by the name then: main window, horizontal
tabs variant, the `+` menu, settings, the lock screen, SFTP), drawn with Quadra's kit
(`quadra-design`) in the kit's design language; OneTerm 0.6 is the reference for behaviour
and settings the design leaves open. The terminal is Quadra's `quadra-term` (Quadra's
specs/18) over the user's engine `oneterm-vt`. The user's decisions and the dependencies
admitted are in `decisions.md`; what is done, in `docs/progress.md`.

A reference written "Quadra's specs/NN" is to a file of Quadra's repository.

## Phases

| Phase | Contents |
|---|---|
| **P1 Local** | the window, tabs and spaces, local shells, the terminal signals, the `+` menu, the status bar, settings, the lock screen |
| **P2 Remote** | SSH over `russh` on a `tokio` runtime of its own with `ring` for its crypto (the user's choice, 2026-11-15; `decisions.md`), saved hosts in the rail, quick connect, the SFTP dock and its expanded view |

P1 milestones, each committed green on the gate before the next:

| Id | Contents |
|---|---|
| M1 | the window and its title bar, one local shell filling the workspace, the status bar with the cwd and the clock |
| M2 | tabs in the rail and spaces side by side in a tab, their keys; the horizontal tabs variant |
| M3 | the signals: OSC 7501 status, OSC 9;4 progress, the bell, OSC 9 toasts |
| M4 | the `+` menu (local shells, split), the status bar's git, CPU, memory |
| M5 | settings (pages, saved as they change) and themes |
| M6 | the master password and the lock screen |

P2 milestones:

| Id | Contents |
|---|---|
| M7 | the SSH engine (`crates/ssh`): connect, host keys, authentication, the shell; quick connect from the `+` menu |
| M8 | saved hosts: the rail's SSH hosts with a filter, the menu's search, Recent and groups, Add SSH host; the host and its latency in the status bar and the space's header; `n ssh · n local` |
| M9 | the SFTP dock (460 px) and its expanded view: listing, moving about, upload and download, transfers |
| M10 | jump hosts and port forwards |

## Window

Crashes: with its folders OneTerm writes a report of a panic (Quadra's specs/15) and, through
`quadra_shell::crash_reports` (Quadra's specs/16), of a crash that is no panic (a minidump and the log's
last lines) to `crashes` in its data folder. The next start shows a warning card, `OneTerm closed
unexpectedly` with the report's name, whose Show report shows the report in the file manager
(`Opener::reveal`); each report is told once (`App::last_crash`). Nothing is sent anywhere.

Custom frame (`WindowOptions::custom_frame`), 1440×900 at most 85 % of the display, centred,
at least 720×480 (`WindowOptions::min_size`; a smaller remembered place opens at it), its place
remembered. Top to bottom: the title bar, a bar of the design language (32 px), and, under the body,
the status bar, lower than it (28 px, the user's decision of 2026-11-25: the title bar is
the taller of the two). Both are the kit's (`titlebar`, `statusbar` and its `status_divider`),
which carry their heights: the app writes neither. Colours are the kit's roles; the accent is the theme's. The look follows
the user's canvas "OneTerm" as revised on 2026-11-16 (the Quadra design language: the UI face at
14 px on 23 px lines, labels 12 px on 19, the kit's mono face for paths, numbers and keys).

- **Title bar** (the chrome plane, a line under it): the mark, a terminal's prompt (the kit's
  `terminal` glyph, 20 px) in the brand's colour `#58C4DC`, the same in every theme, and
  "OneTerm" (13 px, semibold), 12 px from the left edge; then, right, icon buttons of 22 px with
  14 px icons, in this order: the
  bells and notices (M3: a bell and one count of bells not heard and notices not seen in mono,
  `3 bells and notices`, both in the error colour while the count is above 0, else the muted
  colour; 22 px high, 4 px at its sides; a click marks the notices seen), Lock OneTerm now (M6:
  while a lock is set, a lock icon and the minutes left before it locks in mono, `9m`, both
  muted, 22 px high, 4 px at its sides; the icon alone in a box of 22 when none is set),
  Open settings (M5); in the horizontal variant SSH hosts before the bells, so the bells,
  Lock and Settings stand where they do with the rail. No search there: the `+` menu and its
  search are the rail's and the keys'. Then the window buttons, 36 px
  wide, 8 px after. In the settings only Done shows; locked, only "Locked" after the name, in
  the secondary colour.
- **Body**: the rail (270 px, vertical tabs), the workspace, and from P2 the SFTP dock (460 px)
  on the right.
- **Status bar** (the chrome plane, a line over it, 12 px, groups split by 1×14 px lines):
  first Collapse tabs to icons (the rail's fold, an icon button in the accent); then the active
  space's cwd after a folder icon (a click copies it; a shell that has said none has no
  group, as every group with nothing to say: no icon alone, no two lines side by side; a
  remote space's is the server's, which its shell says through shell integration, "M7 in
  detail"); git branch and changes after a branch
  icon (M4: `main ↑2 M1`), a group of its own, of a local space alone (a server's directory
  names no repository of this machine); a red dot of 8 px that blinks
  each half second while the focused space logs; then right the OSC 9;4 meter
  (M3: `build`, a 56 px line, the percentage), a remote space's network (P2; a local one shows
  nothing there, nor one not connected yet): what came and went in the last second, `↓ 12 KB/s ↑ 1.1 KB/s`, counted on
  the connection's socket, jump hosts' traffic in it; the focused space's grid, columns by
  rows in mono (`120 × 32`; `Session::grid`, so it follows the window, a split, the dock, the
  font's size and the focus; none with no space); CPU and memory (M4), the `2026-11-16 20:45:07` (24 h, the local time, again each second; in mono, so only its
  text is painted again), and last Toggle SFTP panel. Labels are the secondary colour, values
  the text colour, numbers in mono. The icons have the kit's roles (the canvas leaves them in
  the label colour): the folder `warn`, the branch `ok`, the server `info`, the lock
  `hue_purple`. The minutes before the lock are in the title bar (Lock OneTerm now).

## Rail (vertical tabs)

270 px on the panel colour (the setting, 140 to 400), a line on its right. A heading of 28 px
("Tabs", 12 px semibold, the secondary colour) with the count (mono, regular, the third text
colour) and Collapse tabs to icons at its right; then, under a strong line, the tabs as the kit's tab rules turned on their side: an item row (28 px) a tab, the full
width, no line between them (`panel_hover` at half under the pointer), 14 px from the left: an icon (the focused space's shell's, below, or a server when a space is
remote; the third text colour unless active), the title, and at the end the tab's signal
(below). The active row is the touch step (40 px) for its second line, the editor colour in the accent's tint (`accent_tint` over `canvas`) and one
pixel wider (over the rail's line), its title semibold, an accent bar of 3 px on its left, a
second line in 12 px and the secondary colour (`2 ssh · 1 local · 3 spaces`), and Close tab at
its end. A tab with a progress has a
2 px accent bar along the row's bottom edge, as long as the progress. Then "New terminal" as one
more row: in it, 4 px from its seams and 6 from its sides, a box of a dashed 1 px strong line
(radius 6, the hover fill inside it) with a `+` 14 px from the rail's edge and `Ctrl-T` at its end; it opens the `+` menu at the rail's right. From P2 the SSH hosts follow: a heading "SSH hosts"
with Add SSH host, the filter ("Filter hosts", 28 px), then groups (a chevron, the name, the
count in mono at the right) of data rows (24 px, radius 6) under a line from the chevron: a 10 px square in the host's colour tag (`Colour tag: red`, `Colour tag: #e06c75`), the
name, the address in mono at the right. The switch to horizontal tabs is in the settings.

Collapsed (Collapse tabs to icons, in the heading or the status bar, or the setting) the rail is
40 px (the canvas's option I): Expand tabs at the top, then a box of 40 px a tab, with no line
between two tabs, as the open rail has none (the user's decision, 2026-11-24; a line of
`line_strong` stands over the first and after the last): its icon of 16 px (the secondary text colour, the accent while it
shows) and 6 px under it its signal as a line of 22 by 4 with round ends: `warn` for blocked,
`error` for error, `ok` for done; its progress as a meter (the accent filled to its part on
`line_strong`, the error colour for an error); working with no progress the kit's busy bar of
that size; a bell rung unseen the error colour broken in two pieces of 9, so it is not error;
else no line and no room kept for one, the icon in the box's middle (a program that said no
state, or is idle). The
one that shows is on the pressed editor colour, a pixel wider, with a short accent bar of 3 px
floating at its left, 8 px from its top and bottom; `panel_hover` at half under the pointer.
New terminal is a `+` in a box of 40 px, a line over and under it. The title and signal are its name
and tooltip (`neo-gateway deploy, blocked, needs permission`). The tooltip shows at the tab's
right, in two lines, the focused space's (its title and signal are the tab's name): its
working directory (`D:\quadra`; while the shell has said none, the program it was started
as, `cmd.exe`), then, smaller and muted, the shell's program and the command it runs, the newest
process the shell started, as its command line (`pwsh.exe · npm run dev`), or with none
running the shell's full path (`C:\Program Files\PowerShell\7\pwsh.exe`; `ssh
deploy@staging-db-01` for a remote space). The command is read from the OS when the tooltip
shows (`quadra_shell::shell_process`, Quadra's specs/16), never while it does not. Then New terminal: a row of 40
under a strong line, in it, 4 px from its seams and 6 from its sides, the expanded rail's box
of a dashed 1 px strong line (radius 6, the hover fill inside it) around the `+`; the `+` menu
at its right.

Click selects; middle closes (collapsed too); double-click renames (in the Rename tab dialog,
as SFTP's Rename: 320 px, a Name field 12 px from its sides, Cancel and Rename; Enter keeps it;
in the strip in place); the last tab closed leaves the window open; the right button opens the tab menu (Rename,
Duplicate, Close, Close others, Close to the right: the tabs after it, asked first as a close
is when one has edits). Rows drag to reorder.

**Horizontal variant** (setting "Tabs: vertical | horizontal"): no rail; a strip of 32 px and a
strong line under the title bar, one tab per button split by strong lines (icon, title, then its signal: the state's icon,
else the progress in mono in the accent with a 2 px bar along the tab's bottom, else a bell, else
nothing), the active one on the pressed editor colour, semibold, an accent line of 3 px along
its top and Close tab at its end; New terminal (`+`, the `+` menu under it) after the last, "Vertical tabs" (Switch to vertical tabs, a
panel icon and the word) at the end. The title bar's SSH hosts button opens the rail's hosts as
a panel of 248 px at the left.

## Spaces

A space's terminal reads its lines by its shell (Quadra's specs/18, `ShellProfile`): `cmd.exe` as
Command Prompt's, `powershell.exe` and `pwsh.exe` as PowerShell's, anything else (Git Bash,
WSL, a host over SSH) as a Unix shell's, by the file name of what it started.

A tab holds one or more spaces laid out by a pane tree (the kit's `work::Pane`, `panes`):
splits in two, nested, side by side or one over the other, with the kit's seams between (a
1 px line, the only room between two spaces, under a strip of 6 px over their edges that is
dragged to give a side more room; a double click makes the
sides even). A tab arranges its spaces by itself until the user lays them out: arranged, its
spaces stand side by side (a row of equal columns); beside the rail, three or more stand as the
first on the left (half the width) and the others stacked at its right in equal heights. The
tree is built again by that rule whenever a space is added (Split space, after the focused one)
or closed, and when the tabs move between the rail and the strip. A split to a side, or a seam
dragged to another place, makes the layout the user's: the tree stays as it is, and is changed
only by what is done to it. A split to a side puts a new local shell in the focused space's cwd
beside it, on the side it names, in half of its room, and the focus goes to it; splits along one
axis that hold one another share their room evenly again (three side by side are thirds), and so
they do after a close. Split space in a layout of the user's is a split to the right. A tab back
at one space arranges itself again. The order of the spaces (the focus keys, the status bar) is
the tree's reading order (in an arranged tab, each added space comes after the focused one). The
layout is not saved. A space is a terminal, or the editor of a remote file (M9, "Editing in
OneTerm"): that one runs no program and is in no input channel; it is not saved either. Each space of a tab of two or more has an 18 px header (a tab of one space has none) in 12 px on the panel colour (a line of 16 over one pixel, so its words stand in the middle of the 17 its line leaves and both faces share a baseline), a 1 px line along its bottom (inside the 18 px): where it is on the left,
in mono (the cwd, after `user@host:` for a remote space; a shell that has said no cwd, its
title after `user@host`, the title alone when it says who already; in the text colour while focused), its
signal on the right (a progress percentage in mono; the state's 12 px icon and a label in the
state's colour, `deploy blocked`, then the latency in mono; with no state the latency, else the
program's title, a program's path shortened to its name as in a tab's title (`cmd.exe - ping
x`); a local space whose program has given no title yet shows its shell's name from the start,
`Command Prompt`, `PowerShell 7`, or a typed program's file name).
The terminal has 10 px of room above and below and 12 at its sides. The focused space has a
1 px accent ring, its own border (nothing lies over the terminal for the pointer). Notification cards stand at the window's bottom right, above the status bar, 448 px wide and 12 px from the edges (over the dock): on the panel plane with a strong line, radius 10 and the second elevation, an info icon, the title, the text in the secondary colour, then `staging-db-01 · OSC 9` in 12 px, and Dismiss notification. `Ctrl+D` splits
the active space (Split space: a new local shell in its cwd, after it, with the focus),
`Ctrl+Shift+Right`, `Left`, `Up`, `Down` split it to that side, `Ctrl+W` closes it (the tab with its last
space), `Ctrl+Alt+Left/Right` moves the focus; a press anywhere in a space makes it the focused one. A space whose program ended shows
`[process exited N]` and closes on a key. The other button opens a space's menu (when the
setting has it, the default): Open link and Copy link first when the press was on a link, Copy
(with a selection), Paste, Select all, Clear scrollback, then Start logging (Stop logging while
a log is open), then Split right, Split left, Split up, Split down, Input channel, and Close space, the groups split by lines, no
icons; a program that reports the mouse takes the press instead.

Input channels (broadcast typing): five, A to E. A space that joined one sends what is typed in
it to the other members of that channel, in any tab: keys as its view encoded them, typed text
and pastes as each member's own program asked (bracketed or not), through
`Session::on_input` and `Session::deliver` (Quadra's specs/18, "Input"); mouse reports, focus reports and
replies are not sent. The space's menu has Input channel, a submenu: Channel A to E (the
space's own checked; choosing one joins it, leaving any other), then while a member Leave
channel and Close channel X (every member leaves), then in a tab of more than one space Join
all spaces in tab to X (while a member) and Leave with all spaces in tab (while one is a member).
It is disabled once the space's program ended. A member's header has a chip before its signal:
a 14 px square, radius 3, its bold letter in 10 px on the channel's colour (the kit's
`token::SERIES`, 1 to 5), read as "Channel A"; while it is the focused one of several, its ring
takes that colour. A space made by a split or a duplicate is in no channel; a space leaves its
channel when its program ends or it closes. Membership is not saved.

Links (Quadra's specs/18, "Input") open on Ctrl+click, or Open link in the menu, through the app's policy,
a pure function of the target and the visible text: `https` opens through the shell's `Opener`;
`http`, a port that is not the scheme's own, or an OSC 8 link whose visible text looks like an
address of another host (a word like "click here" is no address) first asks, in a dialog that
shows the target (and the text when they differ), Open or Cancel; any other scheme, an address
with no host after `//`, a control, space or direction character, a user or password in the
address, or a target over 256 KiB is refused, with a card saying why.

## Find

`Ctrl+F` (the binding `find` of the Keybindings page; bound for the whole window, so the keys
reach a shell only once unbound there) opens a find bar at the top right of the focused space,
over its terminal, and puts the focus in its query; pressed again it gives the focus back to the
query. The bar is one row of the kit's pieces, 4 px inside its plane (Quadra's specs/design/controls.md,
"A find bar"): the query (`input`, the md step, 220 wide or less in a narrow space (96 at least: the bar stays inside it), the search
icon before it, placeholder and name "Find"; the error colour
while it finds nothing), the switches Match case, Whole word and Regular expression as toggled icon buttons (the
icons `case-sensitive`, `whole-word`, `regex`: the accent while on), the count in 12 px of the second
text colour in a box of 64 px so the row does not move ("3 of 12", "No results" in the third,
nothing while the query is empty), a rule, then Previous match and Next match (off while
nothing is found) and Close as icon buttons; on the overlay colour with a line, the middle
radius and the first elevation. Typing finds at once (Quadra's specs/18, "Find"); Enter and F3
go to the next match, Shift+Enter and Shift+F3 to the one before, wrapping; Escape or Close
ends finding and gives the focus back to the terminal. Closed, the bar keeps its query: opened
again, it finds it again. Open, it stays open with its query when the space's view is built
again (a tab shown again, the tabs moved to a strip): it is built from the session's finding
(`Session::find_query`).

## Completion

A space completes the command being typed (specs/completion, the engine; the app makes one `Engine` in
its model when first asked, so the history is the session's, shared by the tabs). The family of a space is
`force_family`, else `Family::detect` of the local shell's program (the OS's: `cmd`), and Unix
for an SSH space. The overlay follows the terminal's `command_line` (Quadra's specs/18) and asks
`Engine::suggest(text, text.len(), family)`: nothing while completion is off, on the alternate
screen (`disable_in_alt_screen`), away from a prompt (`require_prompt_region`), without the
caret, or after Escape until the line changes. It stands under the caret at the start of the
token (above it when there is no room below), on the plane of the kit's popups (its fill,
border, radius and level 1 shadow); data rows (24 px) in the kit's mono face: the matched part in
the accent's text colour, the rest, the option's hint and the kind (`command`, `subcommand`,
`option`, `history`) in the second text colour; the chosen row filled as a menu's. At most
`max_visible_items` rows show, the window following the chosen row, with `↑ N more` and
`↓ N more` above and below.

Keys while it is open, taken before the shell (`Terminal::on_key_first`): nothing is chosen at
first, so Up, Down and Enter go to the shell; Tab (with `accept_tab`) chooses the first row, and
then accepts; with `accept_tab` off Tab is the shell's and Down chooses the first row. Once a row is chosen, Up and Down (and Ctrl+P, Ctrl+N) move it, and Enter accepts.
Escape closes it. Accepting writes `Suggestion::edit`: a DEL for each character to erase, then
the text; a suggestion that does not extend what is typed (`prefix` false) only closes it.
Enter that goes to the shell records the line first (`Engine::record`, which redacts), when
the line is at a prompt (or `require_prompt_region` is off) and not on the alternate screen:
a password typed for a program is no command.

Settings (Terminal page, group Completion; `completion.*` in `settings.json`): enabled (off at first), accept
with Tab, history kept (500, 0 to 5000), characters before suggesting (1, 0 to 10), rows shown
(8, 1 to 20), the history, hand-written catalogs and generated catalogs as sources, fuzzy
matching, the ancestors' options, off in full-screen programs, only at a prompt, Unix commands
in Cmd and PowerShell (off), the shell family (Detected, Cmd, PowerShell, Unix), redact secrets;
all on but where it says. A change applies at once (`Engine::set_config`).

## Signals

| Signal | Tab | Space header | Bar | Else |
|---|---|---|---|---|
| OSC 7501 status | the state's icon at the end, each state its own shape, outlined in its colour (14 px): blocked a `?` in a circle (warn, `Blocked, needs permission`), done a check in a circle (ok, `Done, result not yet seen`), error a cross in a circle (error), working a spinner arc (accent; a progress shows in its place); else nothing: OSC 7501 shows only what a program sent, and idle shows none | the state's icon and a label in its colour: the record's id and the state (`deploy blocked`, `Session::status_name`) | | |
| OSC 9;4 progress | the percentage in mono and the accent at the end, a 2 px accent bar along the row's lower seam | the percentage | `build`, a 56 px line and the percentage | |
| BEL in an unfocused space | a bell icon in the error colour at the end (`Bell rang`) | | the title bar's count | |
| OSC 9, 777, 99 notification | | | the title bar's count | a card at the window's bottom right above the status bar (the title, the text, the tab and the sequence: `OSC 9`, `OSC 777` or `OSC 99`), 6 s or until dismissed (setting) |

OSC 99 in full (`Config::notifications` declares every ability: focus, report, close events,
unfocused, invisible, icons, buttons, close, alive, sounds, urgency, expiry): a notification's
buttons are the card's,
a click on its words opens it, which shows its tab and focuses its space unless it asked not
to (`a=-focus`); either is reported to a program that asked (`a=report`: `Activated`,
`Button`). A card that goes is reported closed to one that asked (`c=1`). One with the id of a
card of the same space that still shows takes that card's place. Critical urgency is the
danger tone and stays until dismissed, low and normal inform; an expiry the program gives
(`w`) replaces the setting's time, `w=0` stays. A program's close shuts its card; its alive
query is answered with the ids of its cards that show.
One that wants to show only unfocused (`o=unfocused`) is dropped while its space has the
keyboard: the window active, its tab shown (no settings page over it), the space its tab's
focus; one only invisible (`o=invisible`) while the window is active or its tab shows and the
window is not minimized.
A covered window counts as seen. Its icon is a name first (`error`, `warn`, `warning`, `info`,
`question`, `help`: the kit's icons of those), else its data, kept under its icon id for later
notifications of the space that name only the id (8 kept, the oldest forgotten). A sound plays
only when the program names one (`system`, `info`, `warning`, `warn`, `error`, `question`; a
name of its own plays the system's): a card in the window is no desktop notification, and the
shell's notification after each command stays quiet. A notification that shows no card
(dropped, the cards silent, the window locked) plays nothing.

Seen clears: selecting the tab clears its bell and a "done" seen state.

## The `+` menu

At the right of "New terminal" (over the rail's edge by 8 px; under the strip's `+`), the kit's
menu: Local shell (`Ctrl-T`), Other shells (a submenu of the local shells found, each with its icon: Command Prompt,
Windows PowerShell, PowerShell 7, Git Bash, WSL, Nushell, MSYS2, Cygwin; the canvas shows one shell), from P2
Quick connect… (a bolt, `Ctrl-Shift-K`), Split current space (`Ctrl-D`, Split space); a line,
Recent (up to 3, how long ago: `2 min ago`, `yesterday`), the saved hosts under their groups'
headings (the address in mono: the host, its port when not 22, `· jump` for a host reached
through another); a line, Add SSH host…. A host's row has the 10 px square of its colour tag in
the icon's place. It has a field over it ("Search hosts, or type user@host…", 360 wide; M4 in
detail), the focus in it as it opens.

## M4 in detail

- **Shells found**: Command Prompt (`%SystemRoot%\System32\cmd.exe`), Windows PowerShell
  (`%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe`), PowerShell 7
  (`%ProgramFiles%\PowerShell\7\pwsh.exe`), Git Bash (`%ProgramFiles%\Git\bin\bash.exe`
  `--login -i`), WSL (`%SystemRoot%\System32\wsl.exe`), Nushell (`%ProgramFiles%\nu\bin\nu.exe`, or
  `%LOCALAPPDATA%\Programs\nu\bin\nu.exe`), MSYS2 (`%SystemDrive%\msys64\usr\bin\bash.exe`
  `--login -i`), Cygwin (`%SystemDrive%\cygwin64\bin\bash.exe` `--login -i`), each where its file
  is (`CHERE_INVOKING=1` keeps a login bash in the directory it starts in). Each kind of shell has
  an icon of its own, the kit's stroked outlines on a grid of 24, known by the shell's name or
  its program's file name: Command Prompt a window with a prompt, PowerShell (both) a slanted
  box with `>_`, Git Bash a diamond with a branch, WSL a penguin, Nushell `nu`, another bash
  (MSYS2, Cygwin, zsh, fish) a box with `$`, anything else the terminal square. The default
  shell is Command Prompt until settings (M5) name another. A shell found is named by its
  path with the OS's separator alone (Command Prompt reads `/cmd.exe` in its own path as a
  switch, `/C md .exe`, and ends). With no tab left (the last one
  closed, however it went) the window stays: the workspace says `Ctrl+T opens a terminal`. A split starts in the focused
  space's directory. A local shell's environment is the user's as the registry says it at
  that moment (`CreateEnvironmentBlock`, as Windows Terminal does: a variable set after OneTerm
  started is in it), over OneTerm's own. It names the terminal (`TERM_PROGRAM=OneTerm`,
  `TERM_PROGRAM_VERSION` its version) and, while Force links is on, adds `FORCE_HYPERLINK=1`:
  OneTerm opens OSC 8 links, and programs that know terminals by name (OneTerm is on no list)
  show links on it; off, a program that writes links even into a pipe or a file does not.
  **Shell integration** (the setting of that name, `terminal.shell_integration`, on): a shell
  OneTerm found is started so that it says where it is and where its prompts, commands and
  outputs begin and end, and writes UTF-8 (`local/integrate.rs`). Command Prompt: `/K chcp
  65001 >nul`, and a `PROMPT` of OSC 9;9 with its directory, OSC 133 `A`, the prompt, `B` (its
  one hook: it cannot say `C` or `D`). PowerShell, both: `-NoExit -Command` with one quoted
  script that sets the console's output to UTF-8, wraps the `prompt` function (`D` with the
  status, OSC 9;9 in a folder of the file system, `A`, the user's own prompt, `B`) and binds
  Enter through PSReadLine to say `C`, calling what Enter was bound to. bash (Git's, MSYS2,
  Cygwin): `PROMPT_COMMAND` (`D` with the status, OSC 7, `A`, `B` put once at the end of
  `PS1`) and `PS0` (`C`); a `PROMPT_COMMAND` of the user's runs after it, and one that says
  OSC 133 itself is left alone. A program the user typed, WSL and Nushell start as they are.
  Off, a shell gets none of it (Command Prompt then says no directory). A directory a bash
  says its own way (`/c/work`, `/cygdrive/c/work`, `/mnt/c/work`) is taken as the folder it
  is (`C:\work`), so a new space starts there.
- **The `+` menu** has a search field wherever it opens, the rail's New terminal (expanded
  or folded) or the strip's `+` (`searched_menu`): what is typed keeps the shells, recent targets and hosts whose
  names or addresses hold it (whatever the case), and a typed target (`Target::parse`) is a
  row `Connect to user@host`: first when it names a user (`@`), else after the hosts found.
  Enter takes the first row.
- **Git**: the branch from the `.git` folder found up from the focused space's directory
  (`HEAD`; a detached head shows its short hash), and the commits ahead of its upstream (`↑2`,
  green) and the count of changed files (`M1`, yellow) from `git status --porcelain --branch`,
  then the lines added (`+12`, green) and removed (`-3`, red) against the last commit, from
  `git diff --numstat HEAD` (a binary file counts none; a file git does not follow yet is a
  changed file and no lines); each shows only when it is not 0. Both are run on the pool with
  no window, again when the directory changes and every 10 s; nothing when there is no
  repository or no `git`.
- **CPU and memory**: of OneTerm's process (the `Meter` in the context, `quadra_shell::install`'s
  or a test's `FakeShell`; with none, `process_usage`; 16), read every 2 s (`CPU 3%`, `RAM
  182 MB`, the private working set): the processor time between two readings over the time the
  process lived between them and the count of processors. The pointer resting on CPU and RAM
  shows the kit's tooltip above them (`.content`), holding two columns split by a line:
  "Memory" (Private working set, Working set, Commit (private bytes), Peak working set, in MB
  with one decimal, `182.4 MB`) and "Processor" (CPU usage `3%`, CPU time and Uptime as
  `h:mm:ss`, Threads), values in mono, following the readings while it shows; a reader is
  told `OneTerm: CPU 3%, RAM 182 MB`. The canvas's
  machine-wide memory and network rates wait for a framework service that reads them.
- **The meter** shows the progress of the tab that shows, else of the first tab that reports
  one; its label is the OSC 7501 id of the space that reports it (`build`; none without one),
  120 px at most.

## Settings

A window of its own (980 by 700, its place kept; the title bar's Settings opens it or brings
it to the front; it closes when OneTerm locks and with the main window; where no window can open,
the harness, the page shows in the main window with Done), the kit's `settings` in its layout of cards: pages with icons, each with
what it is about, groups with titles; the title bar names the page), a search, pages (a row is explained in a reader's words, what it does and not how it is made; the options of a choice are in the same words, and as a choice is saved by its words, one saved by a word since changed reads as the default):
Appearance (and the UI's text size, 75 to 200 %, the terminals' with it: the app's
`TextSize::text`), Terminal, Tabs & spaces (and the tab's title: the program's, a path of a program
shortened to its name, or "Terminal" and the host), SSH & hosts (the count of saved hosts with Add SSH host,
the recent targets with Forget; Keys, below; Connections: the keepalive, on, every 30 s from 5 to 3600, given up
after 3 unanswered from 1 to 20), SFTP (the downloads folder), Notifications & signals, Status bar
(Git, Terminal size (`status_bar.size`, on: the focused space's columns and rows), CPU and memory, Clock,
and frames a second, on: `window().frames()` counted each second), and whether the SFTP dock is
open is kept for each kind of tab that shows (a local shell's, closed at first; an SSH space's,
open): it follows the tab, and opening or closing it is remembered for the kind,
Security & lock (Lock now while a password is set; and whether programs may read the clipboard by OSC 52, off), Terminal's
Logging (log new local shells, log new SSH spaces, both off; the folder, typed or chosen in the
system's dialog, empty for `session-logs` in the app's data folder; the formats shown, fixed; append or overwrite; a file `<name>_YYYY-MM-DD_HH-MM-SS.log` a space,
its name the shell's or the host's, lines stamped in local time; a space's menu has Start or Stop
logging, and a log that cannot be opened is a card), Keybindings (a row an action, known by a stable id: `new_tab` Ctrl+T, `split` Ctrl+D ("Split the space", Split space), `split_right` Ctrl+Shift+Right, `split_left` Ctrl+Shift+Left, `split_up` Ctrl+Shift+Up, `split_down` Ctrl+Shift+Down, `close_space` Ctrl+W, `focus_next` Ctrl+Alt+Right, `focus_previous` Ctrl+Alt+Left, `lock` Ctrl+Shift+L, `quick_connect` Ctrl+Shift+K, `settings` Ctrl+, (the Settings window, or the page where none opens), `dock` Ctrl+Shift+B (the SFTP dock), `select_all` Ctrl+Shift+A, `find` Ctrl+F (the find bar, "Find") and `clear` (unbound: the scrollback) on the focused space, `duplicate` (unbound: the tab), `zoom_in` Ctrl+=, `zoom_out` Ctrl+- and `zoom_reset` Ctrl+0
(the terminal's text size, Appearance's, a point a step within 6 to 48, back to 15), `quit`
Ctrl+Shift+Q (closes the window), and in a terminal `copy` Ctrl+Shift+C and `paste` Ctrl+Shift+V, and through a terminal's history `scroll_page_up` Shift+PageUp, `scroll_page_down` Shift+PageDown, `scroll_top` Ctrl+Shift+Home, `scroll_bottom` Ctrl+Shift+End (Quadra's specs/18, "Input"); each with its `key_input` and the kit's Reset as on the other pages (and the page's Reset all), and a warning on each of two actions that have one keystroke. A setting `keys.<id>` a row: absent, the default; empty, unbound; Reset writes the default back. The keymap is built again from the defaults and these whenever one changes and put in `app_keymap()`; a value that is no keystroke is the default. Ctrl+Insert, Shift+Insert and Tab in a terminal stay as they are), Advanced (the settings file this run saves to, a trial run's folder too, and Show in folder: the file selected in the file manager, or its folder, made if need be, while nothing is written yet), About (the version, the engine, the license).
Saved to `settings.json` in the app's
folder as they change (Quadra's specs/15). Every setting of a page has the
kit's Reset (Quadra's specs/13, `settings`), its numbers in the kit's stepper (`number_input(..).stepper()`), back to the default the app reads it with (`Prefs::load`; the
defaults are those of settings in memory, written once), and each page its Reset all; the master
password, the saved hosts and the recent targets are data, not settings, and have none. Appearance: the theme (System, Dark, Light), and a colour theme over
it (37 of the user's terminal's palettes as files of the kit's theme format, `crates/app/themes`, made by
`.tmp/gen_themes.py` from colours alone; "Default", the kit's, when none: a saved name no theme has reads as it); the terminal's text (font family,
chosen from the families of the machine in a select with a search, the first "Default" for none (the kit's mono face); fallback fonts, families separated by commas, at first "Symbols Nerd Font Mono, Symbols
Nerd Font"; size, 15; weight, Normal and Thin to Black; line height, 1.2; the cell's width, 0 for the advance of `0`, up to
48 px; the padding a side, 5 at the top and the left and 0 at the right and the bottom, each 0 to 64 px, the only room between the grid and the space's edges; ligatures, on; grayscale smoothing, on (no coloured fringes; off, as Windows smooths); semantic
highlighting, on: text in the default colour coloured by what it means, in the colour theme's ANSI colours;
the reference's Auto and On do the same, so it is a switch) and colours over the theme's
(foreground, background, cursor, selection, the gutter's, as `#rrggbb` in the kit's `color_picker`, its swatch and
code the theme's colour while it is empty: the gutter's text and clock the foreground mixed half with the
background, its background the background, its line number `#2b7f99` by default; the minimum contrast, 4.5, from 1, off, to 21; the sixteen ANSI colours, a row a
colour with its normal and bright fields), the view's `TermLook` (Quadra's specs/18). Keys: `terminal.font_fallbacks`,
`terminal.cell_width`, `terminal.padding.top` (`right`, `bottom`, `left`), `terminal.colors.min_contrast`,
`terminal.colors.ansi.red` (`black` to `white`, `bright_black` to `bright_white`), `terminal.ligatures`, `terminal.grayscale`,
`terminal.semantic_highlighting`:
every terminal follows them as they are, its screen kept. Terminal: cursor shape and
blink, scrollback lines, the default local shell ("System default", the OS's; a shell found; or "Custom program": the
program and its arguments typed, `terminal.shell_program`, `terminal.shell_args`), where new spaces start (active space's cwd,
home, which is the default, last used: the directory a local shell said last, kept across runs as
`terminal.last_dir`, home when it is no longer there), Force links (`terminal.force_hyperlink`, on: `FORCE_HYPERLINK=1` for a new local shell), Shell integration (`terminal.shell_integration`, on; "M4 in detail", and a host's shell: "M7 in detail"); Mouse & wheel: the context menu, copy on select, middle-click paste (all on),
the wheel's speed (1, from 0.1 to 10), alternate scroll (on: arrows to a full-screen program),
the view's `TermSettings` (Quadra's specs/18). A choice is saved as its words. Signals: OSC 7501 shown on tab and header / tab only / ignored; OSC 9;4 on tab, status bar; bell: badge and count / badge and sound (the system's
default sound, at once for each bell however the space is looked at) / badge and flash (the
space's terminal tinted the selection colour for 120 ms) / ignore; OSC 9: toast 6 s / until dismissed / silent.

### Startup commands

What is written to a host's shell once it is open is a command of a list, kept in the
settings and chosen by name (the user's decision, 2026-11-24; before, each host had words of
its own, typed in its dialog). Settings, SSH & hosts has the group **Startup commands**
(`startup.rs`; `ssh.startup_commands`, rows of a name and the command): each command with
its name over what is written, **Edit** (the two fields under the list take it) and
**Remove**; then **Name**, **Command** and **Save command**, which adds it, or replaces the
command of that name; with no name a command is named by its own words, and an empty
command saves nothing. A host's dialog has a select, **Startup command**: None, then the
names. A host keeps the name (`ssh.startup`, rows of the host's name and the command's), so
a command changed in the settings is changed for every host that chose it; one removed is
taken from the hosts that chose it, which run nothing. Settings of before version 3 held
each host's own words there: each becomes a command of the list named by its words, once.

### Network

Settings, Advanced has the group **Network** (`prefs/net.rs`, `page/files.rs`), for what
OneTerm fetches over HTTPS, which is its updates; SSH connections do not go through it.

- **Proxy** (`network.proxy`): System, the proxy Windows is set to use, or Manual.
- **Proxy address** (`network.proxy_address`): the HTTP proxy asked when Proxy is Manual, as
  `host:port`; the field is off while Proxy is System, and Manual with no address is the
  system's. An address that names no HTTP proxy makes the check fail as any network failure
  does, with the reason in the log.
- **Verify certificates** (`network.verify_tls`, on): off takes any certificate, for a
  network that opens TLS on the way; the row shows `Not checked` in the danger colour while
  it is off, since an update is then not known to be GitHub's (its digest comes the same
  way).

The updater reads them each time it checks or fetches. Taking over from OneTerm 0.6 takes
the proxy its updates went through (`update_config.json`, `proxy_url`), and never that
certificates were unchecked there.

## Taking over from OneTerm 0.6

OneTerm takes the place of OneTerm 0.6, whose folder is `.OneTerm` in the user's (`import.rs`).
A build of the name before kept its settings in `%APPDATA%\1Term` and its data in
`%LOCALAPPDATA%\1Term`: at a start with the user's own folders each becomes `OneTerm` there,
where there is none yet (`adopt`). One that cannot be moved, as that build still runs, stays,
and the app starts with folders of its own.

 At its first start (the setting `import.oneterm`, set once it was
tried; not in a trial run under `ONETERM_HOME`, nor in a test) OneTerm takes over:

- the sessions of `ssh_session.json`, of any of its versions, as hosts: the label is the
  name (a label met again gets a number after it, `prod (2)`, as sessions are known by an id
  there and hosts by their name here; one without a label is named by its address), the
  group, `user@host:port`, the way in (`private_key` with its key file, `agent`, else the
  password), the jump host by the name its session got, the forwards (local, remote, dynamic;
  the address listened on is this machine's own here), and the colour as the host's tag. A
  host whose name OneTerm already has is left as it is. What OneTerm has no place for is left
  behind: a session's own logging, agent forwarding.
- of `terminal.json`: the font's family and size, the rows of history, the cursor's blink,
  and the keepalive (on, its seconds, how many may go unanswered).

- the saved passwords, with nothing asked: OneTerm's vault is of OneTerm 0.6's format (below,
  "Saved passwords"), so the document of `vault.json` becomes OneTerm's vault as it is, and
  each session's `sealed_password` the secret of its host, as it is. The master password is
  OneTerm 0.6's from then on: the next start opens locked, and it is asked for the first time a
  host with a saved password is connected to. A password comes along for a host OneTerm has
  under the session's name and target and with no secret of its own, and only an envelope
  of that vault, and only for a session whose way in is the password (one left behind
  another way is stale there, and would be tried here as a key's passphrase); with no
  password saved there, the vault is not taken either. Where OneTerm
  has a vault of its own already (a master password set here
  first), it is kept and no password is taken.

Taking over is automatic: nothing is asked, nothing is shown, and nothing in the settings
imports (the user's decision, 2026-11-24; a card said what was taken and the settings had an
Import button before). To the user it is the same app in a newer version: the hosts are in
the rail, and the saved passwords open with the master password as before. The log says how
many hosts and saved passwords were taken and how many passwords could not be. It happens
once: what OneTerm 0.6 saves afterwards is not taken. OneTerm 0.6's files are read and never
changed.

## Updates

OneTerm updates itself through `quadra-updater` (Quadra's specs/22) from the releases of
`vnStrawHat/OneTerm`, this repository: the updater of OneTerm 0.6 looks there, so its last
version updates to 0.7.0, and the app goes on from the same place. Releasing is the user's
act, at a time of their choosing (`decisions.md`); nothing is published by a script. A
release carries `oneterm-<version>-x86_64-pc-windows-msvc.zip`, holding
`oneterm-<version>-x86_64-pc-windows-msvc/` with `oneterm.exe`, the console host, `LICENSE`,
`NOTICE` and `THIRD-PARTY-NOTICES.txt` (`scripts/notices.py`: the console host's notice, the
licences of the kit's icons and faces, and every crate the binary is built from with its
licence files) (`scripts/build-release.sh`, "The repository", which makes the zip and its
`.sha256`, prints the SHA-256, and publishes nothing). OneTerm's versions go on from that
app's: 0.7.0 is the first. The package's own version in `crates/app/Cargo.toml` is the version last released, 0.6.11 until then: the
user bumps it themselves, by the release workflow, so the next release is 0.7.0 (the user's
decision, 2026-11-25). A build before that says 0.6.11 in About and takes 0.7.0 as an update. The executable's icon is that app's (`assets/logo.ico`, its
48 and 96 px images; Quadra's specs/16, "An executable's icon"), which the window takes too.

Settings, About has the group **Updates** (`updates.rs`):

- **Update**: what is known, and one button that is the next step. Nothing asked yet:
  **Check for updates**. `Checking…` (the button off). `OneTerm is up to date.` `Version 0.7.1
  is available.` with **Install**: the package is fetched (`Downloading 0.7.1 42%`, with
  **Cancel**, which leaves the version to install), checked against the digest GitHub
  publishes, and its files put in OneTerm's folder while it runs; no session is touched.
  `Version 0.7.1 is installed.` with **Restart OneTerm**, which starts the new version and
  closes this one, with every terminal and connection, as the row's words say; left alone,
  the next start is the new version anyway. What failed is said in the row in words (the
  network, no package for this system, a package that is not the one published), with the
  detail in the log, and the button checks again.
- **Check at start** (`updates.check_at_start`, on): a start asks once, off the window's
  thread; a newer version shows a card, `OneTerm 0.7.1 is available`, `Install it from
  Settings, About.`; a check that found none or failed says nothing. A trial run under
  `ONETERM_HOME` asks nothing.

After the updater of OneTerm 0.6 put this app over its folder, the console host that one shipped is still
there (`x64/OpenConsole.exe`, an older version) beside OneTerm's own: `conpty.dll` starts the
one beside itself (tried 2026-11-24 with both in the folder), so the older one is only a
file left behind.

The package is fetched to `updates` in the app's data folder, and removed once installed.
Where the app has no folders of its own (a test, a picture of the page) nothing is asked. `main` calls
`quadra_updater::started()` first, which removes what the version before left moved aside.
Not done: pre-release versions, a version skipped on purpose, a proxy of OneTerm's own (the
system's is used).

## Lock

Only with a master password (Security & lock: set, change, remove; nothing of it is kept:
it is the password that opens the vault). Locks after the idle time (10 min default, 1-480)
or at once with `Ctrl+Shift+L` and the title bar's button. Locked: the workspace blurred and
dimmed behind, the title bar usable and saying "Locked", a card where dialogs hang (the
dialog's look: level 2, 440 wide, 80 px from the window top, centred): a 16 px lock in the
accent and "OneTerm is locked" (16 px), "Locked after N minutes idle. Your sessions are still
running." in the secondary colour, the field labelled Password (32 px) with Show password at
its end, then in 12 px `n sessions connected` after a green dot and `n new while locked` after
a bell in the error colour; a footer over a line with Unlock (primary, `Enter` after it) at the
right. The status bar shows only a lock and "Locked, details hidden", the progress meter (its
line and its percentage, without the name of what reports it: that is a detail) and
the clock. Five wrong tries start a back-off (2, 4, 8, 16, 30 s). Sessions keep running.

### M6 in detail

- **The master password** is the one that opens the vault in `ssh.vault` (below, "Saved
  passwords"): there is one exactly while there is a vault, and no hash of it is kept. Set,
  change and remove ask for the current password when one is set, and a new one twice.
  Deriving the vault's key runs on the pool.
- **Idle** is no key and no pointer press or move in the window; it is counted in steps of
  10 s on the app's timers, so it locks within 10 s after the time set.
- **At start**, with a master password, the window opens locked: what is saved is not a
  start away from anyone at the machine, and the password that opens the window opens the
  vault's data key with it.
- **Locked, the workspace shows blurred behind, and is inert** (`Div::inert`, `State::INERT`):
  a displayed node is in the semantic tree, which any process can read through the OS's
  accessibility, so the rail and the spaces are left out of it while locked, and no press or
  Tab reaches them; the lock's card stands over them on a layer that blurs them by 24 px
  (`backdrop_blur`) and dims them with the scrim. What was open over the window when it locks (a menu, a dialog)
  closes as a press outside would close it (`Tree::dismiss_overlays`). The settings page and the notification cards
  are not displayed; the title bar
  keeps its window buttons and says "Locked"; the app's keys do nothing but `Ctrl+Shift+L`;
  the card is a focus scope with the focus in the password field, and neither Escape nor a
  press outside closes it. Unlocking gives the focus back to the focused space. The design's
  "Use Windows Hello" waits until the framework can ask the OS for it.

## SSH (P2)

The engine is the crate `oneterm-ssh` (`crates/ssh`), the one place `russh` and `tokio`
are named: a runtime of two worker threads, made when the first connection starts, behind a
blocking API of the shape of `quadra_term::Pty`: `Ssh::connect(options, output, told,
exited)`, `write`, `resize`, `close`, and `answer` and `cancel` for the Connect dialogs. The UI
thread never waits on the network.

### M7 in detail

- **Target**: `user@host`, `host:port`, `[user@]host -p port`, `-l user`, and a leading `ssh`
  are read by `Target::parse`; no port is 22; no user is an empty one, which `label` writes as
  `host` (so a saved host or a recent row without a user stays without one): connecting asks
  for it in an empty field whose placeholder is the OS's (`USERNAME`), taken when nothing is typed. The user answered is that connection's only (its
  space's header says it).
- **The engine asks the app, never the terminal.** What it asks is an `Ask`, told with its
  progress through `told` (an `Event`: `Connecting(user@host)` for each hop it starts,
  `Ask(ask)`, `Open` once the shell is), from its threads; the app answers with
  `Ssh::answer(Some(text))` or gives up with `answer(None)`. The asks: `User { host, user }`
  (the user is the OS's, to fill in), `HostKey { host, kind, fingerprint }` (`Some` trusts),
  `Password { target, again }`, `Passphrase { file, again }` (`again`: the one before was
  wrong), and `Failed { reason }`, a connection that ended before its shell (`Some` connects
  again from the start, `None` gives up: `[connection closed: reason]` in the space and exit
  status 255). `Ssh::cancel` gives up the connecting under way at once (`cancelled`, then
  `Failed`). What is typed in the space before the shell opens is dropped; the space's last
  size is the PTY's. The terminal shows only what the server sends, the line of a forward
  that cannot listen and `[connection closed: reason]`.
- **Host keys**: `~/.ssh/known_hosts` (the path is an option). A known key goes on; an
  unknown one is asked (`HostKey`, its SHA-256 fingerprint), and trusting it writes it to the
  file; a changed key is never asked: it fails with `the host key of 'host' has changed`, the
  key sent and its fingerprint, and the line and file of the key known, to remove there. A
  certificate fails (`the host gave a certificate, which OneTerm does not check yet`).
- **Authentication**, by the option `Auth`: first `none`, as OpenSSH does (a server that needs
  nothing of the user, an account with no password, lets it in unasked). Then `Auto` (the
  default) tries the agent
  (`\\.\pipe\openssh-ssh-agent`, at most 6 keys), then `~/.ssh/id_ed25519`, `id_ecdsa`,
  `id_rsa` where they are (a passphrase is asked, three times; a file that does not open is
  passed over), then the password, three times; `Agent`, `Key(path)` and `Password` try only
  theirs (a key file that cannot be read or opened fails with why). The password, empty too,
  goes by the `password` method, or by `keyboard-interactive` when the server offers that and
  not `password` (a server of PAM): each of its prompts is answered with the password. After the third wrong
  password: `permission denied`. An RSA key signs with the hash the server names in
  `server-sig-algs`.
- **The Connect dialogs** (the kit's dialog, `crates/app/src/asking.rs`): a remote space
  keeps its connection's phase (`Connecting`, a question, failed, done) with its session, and
  the window shows the focused space's, one dialog for each phase, swapped as the phase moves;
  a space whose connection is done or local shows none. Giving a dialog up (its cross, Escape,
  a press outside) is its Cancel or Close. They start with the connection, whichever way it
  is started: Quick connect, a host or a recent target in the `+` menu; none for a saved host's
  Connect dialog (below), which shows each phase itself.
  - **Connecting** (`Connecting to user@host…`): Cancel (`Ssh::cancel`). It closes by itself
    when the shell opens, and the focus is the space's.
  - **Unknown host**: `The authenticity of host 'host' can't be established.`, `ssh-ed25519
    key fingerprint:` over the fingerprint in mono 12 px, `Trusting it adds the key to
    known_hosts.`; Cancel and Trust and connect. The focus is on Cancel (the kit's first
    button), so Enter never trusts a key.
  - **Sign in**, one field 28 px high: User (`No user was given for host.`, the field empty, the OS's user its
    placeholder), Password (`Password for user@host`, dots), Passphrase (`Passphrase for key file`,
    dots); after a wrong one `Permission denied, please try again.` (or `Wrong passphrase,
    please try again.`) in the error colour over it. Cancel and Connect (`Enter`, also in the
    field). Nothing typed is kept: no password is stored.
  - **Could not connect**: `Could not connect to user@host.` and the reason in the error
    colour, its first letter upper case (the OS's words for a refused or unreachable host,
    `timed out`, `permission denied`, `cancelled`, a changed key
    with its line and file); Close (the focus; the space says `[connection closed: reason]`)
    and Retry (connects again from the start). A changed host key fails again until the known
    hosts are put right: there Close is the answer offered first, Retry beside it.
- **The shell**: a PTY of `xterm-256color` of the space's size, `COLORTERM=truecolor`; a
  resize is a window change; the channel's exit status is the space's `[process exited N]`,
  and a connection that drops is `[connection closed: reason]` and exit status 255.
  Keepalive every 30 s, given up after 3 unanswered. Connecting gives up after 20 s.
- **Shell integration** (the setting of that name, as for a local shell; on): a server's
  shell cannot be started another way, so once it is open it is typed one line
  (`ssh/src/integrate.rs`, OneTerm 0.6's), and the PTY is asked for with echo off so that the
  line is not seen; its last word, `stty echo`, turns echo on again. The line makes bash
  (`PROMPT_COMMAND`, `PS0`, `B` at the end of `PS1`) and zsh (`precmd_functions`,
  `preexec_functions`) say OSC 7 with the directory and OSC 133 `A`, `B`, `C` and `D` with the
  status, the user's own hooks kept after OneTerm's; any other POSIX shell says its directory
  once. It sets `COLORTERM=truecolor` too (an sshd passes on only the variables it is told
  to accept), and starts with a space, which keeps it out of a history that leaves such
  lines out. So the status bar and the space's header show a remote space's directory, and
  Sync CWD has one to follow. A startup command is typed after it. A shell
  that is no POSIX shell (fish, a Windows server's) answers the line with an error: the
  setting is turned off for it. Off, a remote shell is typed nothing and says no directory
  unless its own files make it.
- **Quick connect**: the `+` menu's item (`Ctrl+Shift+K`) opens a dialog (the kit's): "Open a
  one-off SSH session. It is not added to your hosts unless you tick Save as host.", Target (a
  mono field with `ssh` inside it before the text) with what it accepts under it, Auth (Auto,
  ssh-agent, Key file, Password) and Jump host side by side, then one under the other Open SFTP
  alongside (on), Save as host, Open in the current tab as a new space; Cancel and Connect
  (`Enter` after it). Connect starts a tab (or a space) whose header says `user@host`, and opens
  the dock when asked.
- **Tests**: a `russh` server in the test process on 127.0.0.1 (no other host, ever), its
  answers given as the app gives them (`testing::Client::ask`, `answer`): password and key
  authentication, unknown, known and changed host keys, typing, resize, the exit status, a
  connection dropped, a cancel while connecting to a port that never speaks. The app's
  (`tests/asking.rs`) drive the dialogs through the semantic tree against the same servers:
  an unknown host trusted (its key written) or cancelled, a right and a wrong password and
  Retry, a missing user asked, a changed key shown, Connecting closing by itself.
  `crates/shots` draws them (`ask-connecting`, `ask-host`, `ask-sign-in`,
  `ask-failed`) against those servers.

### M8 in detail

- **Hosts** are the setting `ssh.hosts`, a list of `[name, group, target, auth, key file, jump
  host, forwards, colour tag]` (target as `Target::label` writes it; auth `Auto`, `Agent`,
  `Key`, `Password`; the tag is one of the seven named hues, `red`, `orange`, `yellow`,
  `green`, `cyan`, `blue` or `purple`, kept by its name so it follows the theme, or any other
  colour as `#rrggbb`; empty or a word that is neither, none: a colour by the group's name, the
  same for its hosts; saving a host keeps its tag), so the file reads as the page does; `ssh.recent` holds the last 5 targets connected to, newest
  first, with the time in seconds.
- **The rail**, under "New terminal": "SSH hosts" with an Add SSH host button at its right, a
  filter (on the name and the target, whatever the case), then the hosts by group in the order
  first met (a click on a group's heading folds it), each a row of the square of its colour
  tag, its name and its address. A click opens its **Connect dialog**, as do its menu's
  Connect and Connect in a new space. Its head is the host's colour tag (10 px), `Connect to
  <name>` and the group at the right. Under it, in rows of an 88 px label and a value:
  Address (the host in mono, `port N` muted), Jump host (or None), Host key (`Known` with a
  green check and the key's kind and `SHA256:` fingerprint cut to its ends, from the known
  hosts; or `New` with a warning). Then the fields, two to a row: Username (the saved user,
  `Required`, with `This host has no saved username.` under it when there is none; it takes
  `user` or `user@host:port`, and what it gives of them takes the saved one's place for this
  connection) beside Auth method (Auto, SSH agent, Key file, Password; the saved one, `Saved
  for this host.` or `Saved for this host: <auth>` under it); for Key file the key, a select of
  the Keys by their paths (`~/.ssh/id_ed25519`; a host's key not among them is offered too),
  beside the passphrase; for Auto and Password the password across, both with the
  eye that shows what was typed (a saved one is used when it is left empty, the empty password
  else); for Auto and SSH agent a line of what they do. A new key adds a note (`OneTerm has
  not seen this host's key. Its fingerprint is shown to check before it is trusted.`): no key
  is trusted before it is seen, so nothing is ticked here; the engine's question is a step
  of connecting (below), with the host, the key's kind and its fingerprint.
  Two checkboxes end it: `Save username, auth method and password (or passphrase) to this
  host`, which saves what worked once the shell is open (the secret sealed while the vault's
  key is in memory), and `Open in the current tab as a new space` (ticked by Connect in a new
  space). The foot: `Edit host…` at its left, Cancel and Connect (Enter), Connect off without
  a username. The focus is in the first field missing: the
  user, the key file of Key file, the password of Password with none saved. Connecting, the
  body is the steps (Reach jump host, Reach host, Verify host key, Authenticate, each with
  what it is about: a check when done, a spinner under way, a dot to come, by the engine's
  `Connecting` and `Authenticating` events) and what the server asks on the way (a user, a
  password or passphrase, keyboard-interactive's questions, a key) in a field under them with
  `Asked by <name>. OneTerm passes your answer through and does not store it.`, answered by
  Continue or Enter; Connect reads `Connecting…` and is off, and Cancel goes back to the form.
  The tab, or the space beside the focused one, opens only once the shell is open. A failure,
  or a password refused, shows in an alert of the danger tone over the fields (`Could not
  connect` and why), the focus in the password when it was refused. `Edit host…` opens the
  host's editor, and the Connect dialog shows again once it closes. A saved secret whose
  vault is locked asks the master password first, and connects as before. Its menu has Connect, Connect in a new space, Edit… (the host editor,
  filled, titled `Edit <name>`; a new name takes the old one's place), Forget password (with
  one saved), Remove. A group's heading has a menu, Connect all (n): a tab for each of its
  hosts, in their order, the last one shown. The Add dialog
  takes the name (the host when empty), the group, the target,
  the colour tag (the kit's `color_picker`, "By group" while empty: its presets are the
  seven hues as the dark theme has them, its "Default" swatch the group's colour, and a colour
  chosen that is a preset's is saved by the preset's name), the auth and the key file (typed,
  or Choose… in the system's dialog of files, from `~/.ssh` when there is one), and for
  the auths Password and Key file the password or the key's passphrase to save (a password
  field, Password or passphrase; editing, empty keeps the one saved, `Saved` its placeholder). A secret is saved only
  sealed with the master password (below, "Saved passwords"): once one is typed, with a master
  password set but not typed since OneTerm started a Master password field shows under it, and
  with none set New master password and Repeat it, which set it as Security & lock does (it
  also locks the window). A wrong or missing one saves nothing and a card says why. Then the startup command (Startup
  command, `tmux attach -t main || tmux new -s main`): kept in `ssh.startup` (rows of the
  host's name and it), it is written with a carriage return once the shell is open, each time
  it opens (a Retry too), for a saved host's tab, Connect all and Duplicate; Quick connect
  has none. Its
  fields, pickers and selects are all 28 px high. Quick connect has
  "Save as host".
- **The host editor** (Add SSH host, or `Edit <name>` from a row's Edit… or the Connect
  dialog's `Edit host…`), 440 wide: its head the colour tag the host will have and the title;
  Name; Group beside Colour tag; Address (mono; a whole `user@host:port` is taken too) beside
  Port (88 px); Username (`Ask when connecting`, Optional.) beside Jump host; Auth method
  beside its Password or Passphrase (with the eye; for Auto and SSH agent none), then a line of
  what the auth does; the key of Key file, a select of the Keys as in the Connect dialog; the
  master password when one is
  needed (Saved passwords); Startup command, a select of the commands of the settings by
  name, None first ("Startup commands", below); Forwards. The Username and Port filled take the
  place of what the Address says. The foot: `Delete host…` at its left (Edit only), Cancel and
  Save or Add host (Enter); `Delete host…` asks once more (`Delete <name>?` at the form's end,
  Keep and Delete in the danger colour), and deleting forgets its saved password and startup
  command.
- **Keys** (`sshkeys.rs`): the key files hosts sign in with, the setting `ssh.keys` (their
  paths), kept in the group Keys of SSH & hosts: Add key… (the system's dialog, from `~/.ssh`
  when there is one), Find in ~/.ssh (each file there beside a `.pub` of its name), then each
  key: its file's name, its path (`~` for the home folder) and its kind and SHA-256
  fingerprint cut to its ends with `· passphrase` when one locks it (`oneterm_ssh::key_info`,
  read without the passphrase), or `Not found` or `Not an OpenSSH private key`; Remove. No
  dialog picks a file: a host's key is chosen by name from these.
- **Saved passwords** are kept as OneTerm 0.6 keeps them (the crate `oneterm-vault`, `format.rs`), so what it saved
  opens here. The vault in `ssh.vault` is the document of its `vault.json` (schema 1): the
  vault's id of 16 random bytes, the cost of Argon2id (version 19; a new vault takes 64 MiB,
  3 passes, 1 lane; a document asking for less than 19 MiB and 2 passes or more than 1 GiB,
  16 passes and 4 lanes is not derived with), a salt of 16 bytes, and a random data key of
  32 bytes sealed (AES-256-GCM, `quadra_shell::seal`, Quadra's specs/16) under the key the master
  password derives. What is sealed is an envelope: `otv1:` and the padded base64 of the
  id's first 8 bytes, the nonce of 12, the ciphertext, the tag of 16. A seal is bound
  (AES-GCM's associated data) to `oneterm-vault`, a zero byte, the layout's version 1, the
  purpose with its length before it (`key-wrap` for the data key, `ssh-password` for a
  secret), the vault's id, and for a secret the host, the port (2 bytes) and the user of
  its target, each with its length in 4 bytes before it. A host's secret is such an
  envelope in `ssh.secrets` (rows of the host's name and that): one copied to a host of
  another target, or sealed by another vault, does not open. Argon2id is the `argon2` crate
  (`decisions.md`), wiped of its memory after. Settings of before this format (version 0: a
  PBKDF2 hash of the master password in `security.password`, a vault `v1$...`, secrets
  `sv1:` and `sv2:`) are forgotten when they are read: nothing opens them any more, and the
  hash is not left in the file. The data key is in memory only, from the
  first time the master password is typed (setting it, unlocking, a Master password field);
  setting the first password makes the vault, changing it seals the same vault's data key
  again (a new salt, the cost of today), so every secret still opens, removing
  it forgets the vault and every saved secret, once a dialog said so: Remove the master
  password?, `The window will no longer lock, and the 3 saved passwords are forgotten. It
  cannot be undone.` (the count left out with none saved), Cancel and Remove (danger). Connecting to a saved host with a secret: with
  the data key, each first question of its password (the host's own, not a jump host's) or
  its key file's passphrase is answered with it, a Retry included (a wrong one asks in the
  Sign in dialog as before); without it, a dialog Unlock saved
  passwords asks the master password first (Cancel connects without). Connect all asks once.
  A secret is never written plain, logged or shown. In memory the data key, a secret opened
  (`Secret`, its `Debug` hiding it) and the passwords typed are wiped when dropped or cleared
  (`zeroize`): the master password and the secret fields of the dialogs, the Sign in field and
  the lock card's field are wiped, not just emptied, once used; the engine wipes a key file's
  passphrase once it opened the key. What leaves OneTerm's hands is not: a password given to
  `russh` to send, the text fields' own copies while typing.
- **The `+` menu** lists Recent (up to 3, with how long ago) and the hosts by group under
  Quick connect, and Add SSH host, under its search field.
- **Latency**: the engine pings the server (an SSH ping, a request that wants a reply) every
  5 s once the shell is open and keeps the last time it took; the app reads it every 2 s into
  the session's `link_note` (`124 ms`). The space's header shows it at its right.
- **A tab's second line** counts its spaces by kind: `2 ssh · 1 local · 3 spaces`; an
  editor's space is not counted.

### M9 in detail

- **The engine's files**: `Ssh::files()` opens SFTP (version 3, `russh-sftp`) on the same
  connection once the shell is open, and gives `Files`, whose calls block (an app makes them
  on its pool): `home`, `list` (folders first, then names whatever the case; no `.` or `..`;
  at most 100,000 entries), `download(remote, local, progress, cancel)`, `upload(local,
  remote, progress, cancel)`, `download_folder` and `upload_folder` (the same arguments: the
  folder's tree, at most 32 deep, its folders made and its files copied one by one, the
  progress over the bytes of all; a link is copied as what the listing says it is, so a
  folder's link is never followed; a remote name that is not a safe local one fails the
  transfer), `mkdir`, `remove`, `stat` (a path's properties: folder or file,
  whether it is a symbolic link, the rest of what it points to, size, mode, uid and gid with their
  names read from the server's `/etc/passwd` and `/etc/group` where it lets them be read, times). A transfer writes `<name>.part` beside its
  target and renames it once whole, so a target is never half written; a cancelled or failed
  one removes its part. A remote name that would leave its folder as a local path (`/`, `\`,
  `:`, `..`, a control character) is refused.
- **The dock**: at the workspace's right on the panel plane, 460 px at first (300 until
  2026-11-24, when it gained the columns below); its left edge is
  a seam (a 1 px line under a 6 px strip with the resize cursor) dragged to make it 220 to 800
  px wide, kept in the settings (`sftp.dock_width`). Opened and closed by the
  status bar's Toggle SFTP panel; it follows the focused space's connection, and each space
  keeps its own: the folder, its entries, the entry chosen and the transfers stay with the
  space they were made in, and showing a space again (another tab, or another space of the
  tab) shows its dock as it was left, without asking the server; a space first shown lists
  its home folder. A local space: the
  header says `local space · n/a`, the body why it is empty and a button that shows the first
  tab with a remote space (`Show prod-api-01`). Its header (28 px, 12 px semibold in the
  secondary colour): `SFTP`, the host in the third colour, and Zoom SFTP (an icon button: the
  expanded view), a line under it. Under it a bar of 28 px: the folder in a mono field of 22 px
  (12 px), then Go to the typed path (Enter goes too), Refresh remote and Remote options (three
  dots one over the other: a menu of Sync CWD and Show hidden files, each checked while
  on); then a header of the columns, a line under it (the user's request, 2026-11-24;
  `dock/list.rs`), as the expanded view's panes have: Name, Size (64 px), Modified (92 px) and
  Permissions (84 px), the last three at the right; a click on one sorts by it, again the
  other way, its label marked `↑` or `↓`, folders staying first, and the order is the
  expanded view's Remote pane's too. Then `..` and the entries as data rows (24 px) with a line under each, as the expanded
  view's have: a folder
  icon (blue) or a file icon, the name, and in mono 12 px in the third colour the size (none
  for a folder), when it changed (`Nov 16 10:52`) and its permissions (`drwxr-xr-x`, as the
  server says them: `Entry::mode`; sorted as a number). A dock made narrow keeps the name its
  room: Permissions show from 440 px, Modified from 340 px. A click
  selects a row (the press fill and a 2 px accent bar at its left, a file's icon in the
  accent); two on a folder (or on `..`) open it, two on a file download it to
  the user's Downloads folder (`name (1).ext` when the name is taken). The other button gives
  a remote row's menu (Open or Download; for a file Edit…: downloaded to `sftp-edit` in the app's data
  folder, then as the setting Editor says: OneTerm's editor (the default; "Editing in OneTerm" below), or
  opened in the system's app for it or the program the settings name with its arguments
  (`Opener::open_with`), sent back each time it is saved, a card saying so; above the size set, 1 MB,
  asked first; Rename…, Copy path, Properties…, Delete…) and, where no row is, the
  list's (New folder…, Refresh), in the dock and the expanded view's Remote pane alike; a local
  row's menu there has Open or Upload, Copy path, Refresh. New folder and Rename ask the name in a
  dialog (not empty, `.`, `..` or with a `/`); Delete asks first (a folder only when empty).
  Properties… asks the server for the entry again and shows a dialog, Properties, with a
  description list: Name, Type (`Folder` or `File`, `(symlink)` after a link), Size (`2.4 KB (2458
  bytes)`, `-` for a folder), Modified, Accessed (`2026-10-28 10:52` where the user is), Permissions
  in mono (`-rw-r--r-- (0644)`: the kind, rwx with `s`/`S` and `t`/`T` for the special bits, the
  octal), Owner and Group (`name (id)`, the id alone without a name, `-` without either), Path (in
  mono, with a copy button), and Close. It shows; it does not change the mode. A failed ask is a card.
  Files and folders dropped from the OS upload to the folder shown; a folder's Download and a
  local folder's Upload carry the whole tree. An upload onto a name the folder shown
  already has asks first (Replace, 360 wide: `notes.md is already in /srv/www. Replace it on
  the server? It cannot be undone.`, several by their names; a folder there keeps its other
  files): Skip sends none of those, Replace (the danger button) sends them over what is
  there; what was not there went at once. What saving an edited file sends back is not
  asked. Transfers wait their turn: one runs at a time
  in each space's dock, in the order asked; one waiting says `queued`, and its cross takes it
  off. Transfers show
  under the list over a line, 12 px, each the dock's whole width: an arrow, `Uploading name` or `Downloading name`, its state
  in mono (`48%`, `done`), a cross that cancels, and a bar of 2 px; a failed one says why. Names
  that start with `.` show only while Show hidden files is on (both views).
- **The expanded view**: the dock's Zoom SFTP (and the title bar's Collapse SFTP, back)
  gives SFTP the workspace; the spaces stay mounted and running, not displayed, and the
  title bar's chip adds `SFTP expanded`. Its header (28 px on the panel plane, 12 px): `SFTP`,
  the host, `ssh session keeps running` in the third colour, and Zoom out (in the accent). Two panes, Local at the left and Remote at the
  right, each with a bar of 28 px: its label, its path in a mono field of 22 px, Go to the typed
  path, Refresh and its options (Local options, Remote options: three dots one over the
  other, the menu the dock's bar has; Sync CWD is the Remote pane's alone: the remote folder
  goes to the focused space's directory when it is a remote path, in the dock too, while
  the dock is open; Show hidden files is one switch for both panes and the dock); a header row (Name, Size 80 px, Modified 124 px, and in the Remote pane
  Permissions 92 px, a local file having none, those after the name at the right;
  a click sorts the pane by it, again the other way, its label marked `↑` or `↓`; folders stay
  first; each pane by name at first)
  and data rows (24 px) with a line under each: `..`, then the entries, size and time in mono 12 px
  (`Nov 16 10:52`); a click selects a row (the accent at 10 % and a 2 px accent bar), two open a
  folder. The Local pane starts in the downloads folder (the setting or Downloads). Between them
  a column of 48 px on the panel plane: bordered square buttons of 28 px, Upload selected,
  Download selected (into the Local pane's folder, a free name), and Compare, which marks a
  remote file whose local namesake is newer `older` and one that is newer `newer` (12 px, in
  yellow). The transfers under both: `Transfers`, `n active · n done`, the rate of those running
  in mono at the right; a row each with an arrow, `name to folder` (the folder in mono), a bar
  of 200 px, its percentage and the cross. The kit draws all of it (tokens, `checkbox`, inputs,
  icon buttons); the canvas gives the layout only.
  `Entry::modified` is the server's mtime (seconds; 0 when it has none).
- **Editing in OneTerm**: Edit… opens the file in a space after the focused one of the tab it
  was chosen in (as Split space puts one), with the focus; the same file open already takes the
  focus instead. The file is read on the pool: a NUL in its first 8 KB, or bytes that are not
  UTF-8, make it binary, and the space says `Binary file, 3 B: not shown, and not edited here.`
  in the third colour. Text is in the kit's code editor (`quadra-code`, mono 13 px, lines of 20)
  coloured by its type: the file's name (`config.yaml`, `Dockerfile`), else the program a first
  line of `#!` names (after `env`: `python3`), else plain; the gutter marks the lines changed
  against the base, the text as it was opened or last saved. Its 18 px header, as a terminal's:
  `user@host:/full/path` in mono on the left (too long, its folder is cut with an ellipsis and
  the file's name stays whole: `test@vm:/home/te…/config.yaml`), then the type (`yaml`, `python`, `text`) in the
  third colour, `● Modified` in the warning colour while the text is not the base (`Saving…`
  while it is sent), Diff (toggled: the kit's `diff` of the base and the text, three lines of
  context, in the editor's place, the editor kept with its undo; `No changes.` when there are
  none) and Save (disabled with no changes), ghost buttons of 18 px; before them Wrap
  (toggled: the setting "Wrap long lines", `sftp.editor_wrap`, off at first, under Editor in
  the settings: a long line goes on to the rows under it at the space's width, the kit's soft
  wrap; while it is off a long line scrolls sideways, by the wheel with Shift or turned
  sideways, and with the caret). Ctrl+S (the context
  `file_edit`) or Save writes the text to the copy in the data folder, with the line ends the
  file had, and uploads it on the connection the file came from; a card says it was saved or
  why not, and once saved the text is the base. A right click opens its menu (while the
  settings show menus): Copy, Paste and Find, the editor's own commands as its keys do them
  (Ctrl+C, Ctrl+V, Ctrl+F), then Close (Ctrl+W, asking as below); a binary file's, or one
  while the diff shows, has Close alone. Closing it with changes (Ctrl+W, Close space)
  asks in a dialog, Save changes?: Cancel, Don't save (it closes), Save (it closes once the
  file is on the server; a failure leaves it open). Closing its tab, Close others, Quit or the
  window's close ask the same, one editor with changes at a time, its tab shown and the space
  focused (out of the expanded SFTP view and the settings); once it is answered the closing
  goes on and the next one asks; Cancel, the cross, Escape or a save that fails stop it, and
  every Don't save of that closing is taken back. Locked, the question waits for the unlock.
  Its session is never connected and keeps the connection of the file, so the dock follows it
  and shows the dock of the space the file came from; it is no session connected on the lock
  card, and the tab's title is the file's name.
- **Sessions keep app data**: `Session::keep(Rc<dyn Any>)` and `kept::<T>()` (Quadra's specs/18), how
  the app finds a remote space's `Ssh`.

### The console host

OneTerm's folder holds `conpty.dll` and `OpenConsole.exe` beside `oneterm.exe` (`decisions.md`):
`oneterm-vt` uses them before the system's host, which passes about half as much output
(Quadra's specs/14, "Terminal throughput"). `scripts/conpty.sh` fetches the pinned package into
`.cache/conpty` at the workspace's root, checks it and copies the two files to
`target/debug`, `target/release`, `target/dist` and `target/dist/examples`, or to the
folders given; `scripts/build-release.sh` puts them in the package, and the licence's notice
(`scripts/notices/conpty.txt`) into its notices. Without them every space still runs, on the system's host.

### M10 in detail

- **Jump hosts**: `Options::jumps`, at most 4 hops, each a target and its auth. The engine
  connects to the first by TCP, and to each next one through a `direct-tcpip` channel of the
  one before; every hop has its host key checked and its questions asked in the Connect
  dialogs like the target's. An error on a hop is said `jump host user@host: ...`; the hops close with
  the shell, the target first. A saved host names another saved host as its jump host (a
  sixth word of its row; a host it names that is gone is no jump), shown in the menu as
  `· jump`; Quick connect and Add SSH host choose one among the saved hosts.
- **Forwards**: `Options::forwards`, each a `Forward` of a way (`Way`), the port listened on,
  and a host and port. Written as OpenSSH's flags are, without the dash:
  - local, `8080:localhost:80` (or `L8080:…`): the engine listens on 127.0.0.1:8080 and opens
    a `direct-tcpip` channel to `localhost:80` as the server sees it for each connection;
  - remote, `R9000:localhost:3000`: the server is asked (`tcpip-forward`) to listen on its
    loopback port 9000; each connection it opens back (`forwarded-tcpip`) is carried to
    `localhost:3000` as this machine sees it; a channel for a port not asked is refused;
  - dynamic, `D1080`: a SOCKS5 proxy on 127.0.0.1:1080 (no auth, CONNECT, IPv4, IPv6 or a
    name), each connection a `direct-tcpip` channel to the address asked, so it goes out from
    the server; another command, or a channel refused, is answered with SOCKS's error.
  A forward that cannot listen, or that the server refuses, is one line in the space; the
  shell stays up. A saved host keeps its forwards as a seventh word joined by spaces, typed
  so in Add SSH host (Forwards, `8080:localhost:80 R9000:localhost:3000 D1080`).

## The repository

OneTerm's repository is github.com/vnStrawHat/OneTerm, the one OneTerm 0.6 was in. One
commit, marked as a breaking change, moves that app's source, as it was at the tag
`v0.6.11`, into the folder `.archived/` and puts this app in its place (the user's
decision, 2026-11-25). The repository, its tags and its releases are kept, so the updater
of OneTerm 0.6 finds 0.7.0 where it looks.

`.archived/` is kept to read, file for file as it was: it is a workspace of its own that
nothing builds, the workflows in it run nowhere, and the gate (`scripts/check.sh`) leaves
it out. Nothing in it is edited.

| Crate | Package | What it is |
|---|---|---|
| `crates/app` | `oneterm-app` | The app: its library (`oneterm`, what tests and the shots tool drive) and its binary `oneterm`; its themes, its icon, its tests |
| `crates/ssh` | `oneterm-ssh` | SSH and SFTP ("SSH (P2)") |
| `crates/completion` | `oneterm-completion` | Command completion (`completion.md`) |
| `crates/vault` | `oneterm-vault` | Saved passwords: the vault's document and envelopes, sealing and opening; it knows nothing of the app ("Saved passwords") |
| `crates/shots` | `oneterm-shots` | The app's screens drawn to pictures without a window |

The names are those of OneTerm 0.6's crates where it had the same part. What that app had as
crates and OneTerm has not is Quadra's now (the terminal's view, the theme, the updater) or
the engine's own repository (`oneterm-vt`); the app's views stay one crate, as they share one
model.

**Quadra** is taken by git at one commit: the root manifest's `[workspace.dependencies]`
names each crate of Quadra's the app's crates take, all at the same `rev`, and the crates
name them with `workspace = true`, as they do each other. Moving to a newer Quadra is
changing that commit in the one file and running the gate. The terminal engine comes with
`quadra-term`, at the tag Quadra pins. A change the app needs in the framework is made in
Quadra's repository first.

**Manifests.** A crate inherits its edition, licence and lints from the workspace. The app's
version is written in `crates/app/Cargo.toml`, the one place a release reads and bumps; the
other crates are never published and take the workspace's. The lock file is committed, and
the gate and the CI build with it as it is.

**Scratch files.** Tests write under `.tmp` of the crate they run in, or of the repository's
root (`crates/<name>/../../.tmp`); downloads go to `.cache`; a release is staged in `dist`.
All three are ignored by git, and nothing is written outside the repository but cargo's own
build output.

**The root's files.** `Cargo.toml` (the workspace of `crates/*`, the keys the crates inherit,
the lints, the profiles: `dist`, and the vault's two crates optimized in a debug build),
`Cargo.lock`, `rust-toolchain.toml` and `rustfmt.toml` (as Quadra's), `deny.toml`, `LICENSE`
and `NOTICE` (which a release carries), `README.md`, `CLAUDE.md` (the working notes),
`.gitignore`, `.gitattributes`.

**Scripts** (`scripts/`, bash and Python; no PowerShell). They find the workspace's root
through cargo; `CARGO` names another cargo than `scripts/cargo.sh`:

- `cargo.sh <args>`: cargo from any folder, with the build's temp files in `.tmp`.
- `check.sh [prod]`: the gate. No PowerShell, no source file over 300 lines, the design
  language's ladders in the views (`check_design.py`, the ladders of Quadra's design
  language), the release notes script's own cases, then `cargo fmt --check`, `cargo clippy -D
  warnings` and `cargo test` with the lock file as committed; `prod` also builds and tests
  in release. It runs checks of the machine kept out of git, where there are any.
- `build-release.sh`: the release asset. The binary in the `dist` profile with
  `scripts/app.manifest` embedded (the manifest and the icon are checked to be in it,
  `exe_icon.py`), staged with the console host, `LICENSE`, `NOTICE` and the notices in
  `dist/oneterm-<version>-<target>/` at the workspace's root, zipped beside it as
  `oneterm-<version>-<target>.zip`, with `oneterm-<version>-<target>.zip.sha256` (the digest,
  two spaces, the zip's name: what `sha256sum -c` reads). The names and the folder `dist`
  are those of OneTerm 0.6's releases.
- `conpty.sh` ("The console host"), `notices.py` ("Updates"; it reads `cargo metadata` on
  its input, and finds the kit's licences where cargo has the kit).
- `release-notes.py <range> [previous tag]`: a release's notes from the subjects of the
  commits: Conventional Commits grouped by type, breaking changes first, each linked to its
  commit, the commit that bumps the version left out (`--check` runs its own cases).

**Workflows** (`.github/workflows`). They follow OneTerm 0.6's, so that releasing stays what
it was, and differ where the app does: one target (Windows x86_64), steps in bash, and the
version read from `crates/app/Cargo.toml`.

- `deny.toml`, at the root: the dependency policy `cargo deny check licenses bans advisories
  sources` holds the graph for Windows to, as OneTerm 0.6's did. Licences: the permissive
  ones the graph has (Apache-2.0, MIT, BSD-3-Clause, ISC, Zlib, Unicode-3.0), the workspace's
  own crates checked too; another licence fails until it is decided there. Bans:
  `portable-pty` and `openssl-sys`; two versions of one crate warn. Advisories: RustSec's,
  unmaintained failing for what the workspace names itself, and one ignored with its reason
  (RUSTSEC-2023-0071, `rsa`, which comes with `russh` at a pinned version). Sources:
  crates.io, and by git Quadra's repository and the engine's, nothing else.
- `ci.yml`: on a push to `main` and on a pull request that touch what is built; a newer run
  of a ref cancels the one in progress. One job on Windows runs the gate
  (`scripts/check.sh`); a second runs cargo-deny over `deny.toml`, by the action OneTerm
  0.6's workflow pins.
- `release.yml`: run by hand with a bump (patch, minor, major) or by a pushed tag `v*`.
  *prepare* works out the version (by hand: the app's bumped, or the app's again when the
  head is a release commit that was never published; by a tag: the tag's, which the app's
  must be). *build* writes the version into the manifest (by hand), runs
  `scripts/build-release.sh` and keeps the zip and its `.sha256`. *release* writes
  `SHA256SUMS`, the notes since the release published before, and, by hand, commits the
  manifest and the lock file as `chore(release): v<version>` on top of the branch as it is
  then; it creates the release `OneTerm <version>` at the tag `v<version>` with the zip and
  `SHA256SUMS`. Actions are pinned by commit, the same commits OneTerm 0.6's workflows pin.
  A bump starts from the manifest's version, which is the version last released: from
  0.6.11, a minor bump releases 0.7.0.

The engine's repository and Quadra's must be readable by the runner. A run of either
workflow has not been seen yet (`docs/progress.md`).

## Tests

The app's tests drive it through the semantic tree on the headless platform with `echo()`
sessions: tabs and spaces made, split, closed, moved; the signals on the rail, headers and
status bar from bytes fed to a session; the bars (`tests/bars.rs`: the bells between Search and
Lock in both variants and gone while locked and in the settings, the clock by a time the test
sets with only its text painted again, the usage and its card from `FakeShell`'s meter); the
menu; completion (`tests/complete.rs`: prompts
marked by OSC 133, the overlay under the caret, the keys, what is written, the history, the gating); find (`tests/find.rs`: Ctrl+F opens the bar, the count, Enter and Shift+Enter, Escape gives the focus back); links (`tests/links.rs`, the shell faked: https opens, http asks and opens on Open, `javascript:` is refused with a card); splits (`tests/splits.rs`: a tab arranged in the rail and the strip, each side, nesting, the order, a seam dragged, a close, Split space in a layout of the user's); input channels (`tests/channels.rs`: join, what a member and a non-member get, a paste bracketed by each, leave, close, an ended program); the Connect dialogs (`tests/asking.rs`, against the engine's test servers on 127.0.0.1); settings written; lock and unlock. Real
shells only in `quadra-shell`'s pseudo console test.
