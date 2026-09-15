# High-Level Design: Prevent terminal completion from slicing strings at invalid UTF-8 boundaries when matching prefixes from history or catalogs.

Intake: IN-0009
Lane: tiny
Date: 2026-08-14

## Idea

Keep the existing byte-length-based prefix semantics, but obtain the candidate prefix through Rust's boundary-checked `str::get`. If the typed prefix length does not land on a UTF-8 character boundary in the candidate, the strings cannot be equal prefixes and matching returns `false` instead of panicking.

## Diagram

```text
typed prefix byte length
          |
          v
candidate.get(..length) -- None --> non-match
          |
        Some
          v
existing case-sensitive / ASCII-case-insensitive comparison
```

## Data Flow

1. Command, subcommand, option, or whole-line history gathering passes a candidate and typed prefix to the shared matcher.
2. Empty prefixes continue to match.
3. The matcher asks the candidate for a prefix ending at the typed prefix's byte length.
4. A short candidate or non-character-boundary endpoint returns no prefix and therefore no match.
5. A valid prefix uses the existing shell-family case comparison unchanged.

## Detail Design

Detail design is **required for the high-risk lane** and optional otherwise. When present, add one file per concern under `low-level-design/` so each stays reviewable.

- [x] Detail design: not needed
- Reason: this is a localized standard-library bounds check with no architecture, persistence, security, or public-contract change.
