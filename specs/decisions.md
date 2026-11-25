# Decisions

What the user decided about OneTerm, and what the app depends on. A decision is the user's;
the specs and the code follow it.

## The user's decisions

| When | Question | Decision |
|---|---|---|
| 2026-11-15 | The app | OneTerm is built again on Quadra, the user's UI framework, over the user's terminal engine `oneterm-vt`; its look is the user's design canvas, and OneTerm 0.6 is the reference where the design is silent |
| 2026-11-15 | SSH and SFTP | `russh` on a `tokio` runtime of its own, with `ring` for its crypto |
| 2026-11-15 | The console host | A release ships Microsoft's newer `conpty.dll` and `OpenConsole.exe` beside the binary, for throughput |
| 2026-11-24 | Name | OneTerm. The work was named 1Term until this day; "OneTerm 0.6" names the versions before 0.7, on another toolkit. The folders kept under the old name are taken along |
| 2026-11-24 | Licence | Apache-2.0, as OneTerm 0.6's |
| 2026-11-24 | Saved passwords | The vault is OneTerm 0.6's format (Argon2id); its vault and sealed passwords are taken as they are, with nothing asked |
| 2026-11-24 | Taking over from OneTerm 0.6 | Automatic, once, at the first start: nothing is asked, nothing is shown, and nothing in the settings imports |
| 2026-11-24 | The executable | `oneterm.exe` with OneTerm 0.6's icon; the title bar's mark is the prompt glyph in the brand's colour, as OneTerm 0.6 has it |
| 2026-11-24 | Updates | Through Quadra's updater, from this repository's GitHub releases, where the updater of OneTerm 0.6 looks |
| 2026-11-24 | Other systems | Windows x86_64 alone; Linux and macOS are left for later |
| 2026-11-24 | Shipped code and the OS's programs | Shipped code starts no program that Windows may not have (no `tar.exe`, no `curl.exe`): an API of the OS or an admitted crate does the work |
| 2026-11-24 | Settings file | `settings.json` is written as objects in objects, not as dotted keys |
| 2026-11-25 | The repository | OneTerm lives in github.com/vnStrawHat/OneTerm, the repository of OneTerm 0.6: one commit marked as a breaking change moves that app's source into the folder `.archived/` and puts this app in its place, the repository, its tags and its releases kept |
| 2026-11-25 | Crates | A set of crates named as OneTerm 0.6's were where the part is the same: `app`, `ssh`, `completion`, `vault` (`oneterm.md`, "The repository") |
| 2026-11-25 | The CI | It keeps the flow of OneTerm 0.6's, the dependency policy job (cargo-deny: licences, bans, advisories) included |
| 2026-11-25 | Versions | The next release is 0.7.0, and the user bumps to it themselves: the manifest has the version last released (0.6.11), and the release workflow's bump writes the new one. Publishing a release, and making its tag, are the user's acts |

## Dependencies

Few, not zero. What the app is built on is Quadra and what Quadra brings: the engine
`oneterm-vt`, the `windows` crate, the accessibility bridge, `serde_json`, an inflater, a
small regex engine (Quadra's specs/01, R15). Beside them, OneTerm's own crates name these, and
a new one needs a row here first and must pass `deny.toml`:

| Crate | For | Behind |
|---|---|---|
| `russh` (features `ring`, `rsa`), `russh-sftp`, `tokio` (`rt-multi-thread`, `net`, `time`, `sync`, `io-util`) | SSH and SFTP: the protocol, its crypto, the agent and the file protocol. `tokio` is an async runtime, and `russh` must have a crypto backend that builds C and assembly (`ring`, by `cc` with the MSVC the target needs anyway): the user's choice (2026-11-15). The tree is about 215 crates. Removing them costs SSH, or an engine of our own | `oneterm-ssh` only |
| `zeroize` (no default features, `alloc`) | Wiping a passphrase from memory once it was used: writes the compiler may not remove as dead. No dependency; in the tree under `russh` already | `oneterm-ssh` (the app takes Quadra's re-export) |
| `argon2` (no default features, `zeroize`) | The key a master password derives for the vault of saved passwords: Argon2id, the function OneTerm 0.6 sealed its vault with, so its saved passwords open here with nothing asked; the OS's crypto has PBKDF2 alone. It brings `blake2` and `base64ct`, all three in the tree under `russh` already. Removing it costs the saved passwords of who comes from that app, or an Argon2 of our own | `oneterm-vault` |

Beside the crates, one binary: **Microsoft's console host** (`conpty.dll` and
`OpenConsole.exe` of the NuGet package `Microsoft.Windows.Console.ConPTY`, MIT, version
1.25.260930003, the package's SHA-256 checked by `scripts/conpty.sh`), put beside
`oneterm.exe`, where `oneterm-vt` finds it before the system's. The user's choice
(2026-11-15), for throughput: the system's host passes 336 fps of the fire demo at 147 by 40,
this one 627 (Quadra's specs/14). Removing it costs half of that; a terminal still works on
the system's.

No crate of OneTerm's has `unsafe` code (the workspace's lints forbid it): what needs it is
Quadra's, at the OS boundary.
