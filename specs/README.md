# OneTerm's specs

What OneTerm does, written before the code that does it: a change starts here, then the
code, then `docs/progress.md`.

| File | What it covers |
|---|---|
| [oneterm.md](oneterm.md) | The app: phases and milestones, the window, the rail of tabs, spaces, find, completion in a space, signals, the `+` menu, settings, taking over from OneTerm 0.6, updates, the lock, SSH and SFTP, the console host, the repository, tests |
| [completion.md](completion.md) | The completion engine: families of shells, catalogs, the line at the cursor, ranking, history, redaction |
| [decisions.md](decisions.md) | The user's decisions, and the dependencies admitted with what each is for |

OneTerm is built on Quadra (github.com/vnStrawHat/Quadra). What the framework does is in
Quadra's own specs, named here as "Quadra's specs/NN": the app's services (15), the shell's
services (16), the terminal view (18), the updater (22), and the design language every view
follows (`specs/design`: its `layout.md`, `patterns.md` and `checklist.md`).

Code names a section as `specs/oneterm, "Updates"` or `specs/completion, "History"`.
