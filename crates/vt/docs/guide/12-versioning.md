# 12. Versions, MSRV, and what to pin

The crate is not published to a registry. Other projects depend on it by git,
and the promise below applies to a git tag exactly as it would to a published
release.

## What to pin

Pin a commit:

```toml
[dependencies]
oneterm-vt = { git = "https://github.com/vnStrawHat/OneTerm", rev = "<commit sha>" }
```

Use the sha of the merge commit you want. Tracking a branch instead builds
whatever landed this morning, which is not what you want in a build you expect
to reproduce. Tags become the better form from the next release onwards: every
existing tag predates this crate, so none of them can carry it yet.

The whole repository is checked out by a git dependency, not just this
directory, so expect the first build to fetch a few tens of megabytes.

## The promise

The crate is `0.x`, and Cargo treats a minor bump of a `0.x` crate as breaking.
So does this crate:

1. A removal, a signature change, or a new variant on an **exhaustive** enum is
   a **minor** bump, with a changelog entry naming the item.
2. A new method, a new module, a new default-off feature, or a new variant on a
   `#[non_exhaustive]` enum is a **patch** bump.
3. Raising the minimum supported Rust version is a **minor** bump. Never a
   patch.
4. `parser::Dispatch` is the engine's internal semantic contract rather than an
   embedder-facing trait; its method set changes at patch level.
5. Anything reachable only with a non-default feature carries the same promise
   as the default surface. A feature is never a stability escape hatch.
6. Behaviour is not the API, with one exception: **the reply bytes for `DA1`,
   `DA2`, `DSR`, `DECRQM`, `XTVERSION` and the OSC colour queries are a
   contract**, because programs parse them. Changing one is a minor bump and an
   entry, even though no Rust signature moved.
7. `FeedStats` counter semantics are documented per field and are part of the
   contract. A counter that starts counting a different thing is a minor bump.

Not promised: grid internals reachable through `grid::Screen`, the exact
sequence-number values, allocation behaviour, and performance.

**The row-identity guarantee is promised**: a `RowId` names the same content for
as long as that content is live. Embedder caches are keyed on it, so it is a
contract rather than an implementation detail.

## Which types are `#[non_exhaustive]`

Ten public types are marked, and the mark costs a caller two different things
depending on whether it is an enum or a struct. The compiler will tell you
either way, but it is worth knowing before you design around them.

**Seven enums**, on which a `match` needs a wildcard arm: `VtEvent`,
`OscRoute`, `Progress`, `ShellMark`, `input::KeySpec`, `input::NamedKey` and
`search::SearchPattern`. A new variant on any of them is a patch release, and
your wildcard arm is what makes that true for your code too.

**Three structs**: `search::SearchOptions`, `ResizeOutcome` and `Placement`.
There are no variants and no wildcard arm; what the mark costs you is that you
cannot build one with a struct literal. `ResizeOutcome` and `Placement` are only
ever returned to you -- by `Terminal::resize` and `Terminal::placements` -- so
you read their fields and never construct one. `SearchOptions` you do build, and
the way to build it is to start from the default and assign:

```rust
use oneterm_vt::search::SearchOptions;

let mut options = SearchOptions::default();
options.whole_word = true;
assert!(!options.case_sensitive);
```

A new field on it is likewise a patch release, and code written that way keeps
compiling across it.

`SearchPattern` is the one where it matters immediately rather than eventually:
its variant set depends on whether the `regex` feature is on, so the wildcard
arm is what lets the same code compile in both builds.

```rust
use oneterm_vt::search::SearchPattern;

// This function compiles with the `regex` feature on and with it off.
fn kind(pattern: SearchPattern<'_>) -> &'static str {
    match pattern {
        SearchPattern::Literal(_) => "literal",
        _ => "a variant this build was not compiled against",
    }
}

assert_eq!(kind(SearchPattern::Literal("needle")), "literal");
```

## MSRV

The minimum supported Rust version is **1.96.0**, declared in the manifest.
Raising it is a minor bump with a changelog entry.

CI builds on the pinned toolchain rather than on the MSRV itself, so treat the
number as a statement of intent checked by hand rather than by a job. If you
need it verified for your own build, verify it in yours.

## Features and what they cost

| Feature | Default | What it adds |
| --- | --- | --- |
| `pty` | on | the `pty` module: a child process behind a pseudo-console. Adds `polling`, plus `windows-sys` on Windows or `libc` on Unix |
| `vt-paranoid` | off | a whole-history integrity walk after every `feed` and `resize`; for tests and fuzzing, never for a release build |
| `regex` | off | `search::SearchPattern::Regex`, and with it the `regex` crate and its three dependencies |

No feature changes behaviour -- only availability. Two of them add
dependencies; turn both off and you are back to six leaf crates, no build
script, no proc macro and no platform code.

`polling` is this crate's **only public dependency**, and only under `pty`:
`Poller`, `Event` and `PollMode` appear in the transport trait signatures, so a
`polling` major bump is a breaking change here and gets a changelog entry naming
both versions.

## The changelog

`CHANGELOG.md` beside this crate is the authority, in the Keep a Changelog
shape. An entry belongs there when it changes what an embedder compiles against
or what bytes the terminal replies with.

The version number is shared with the application this engine was written for,
so a release can carry no API change at all. Such a release says so in the
changelog rather than being omitted, because an embedder reading the file needs
to be able to tell "nothing changed" from "nobody wrote it down".
