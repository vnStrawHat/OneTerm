# 08 — Security & redaction

> Part of [Terminal auto-completion design](../auto-completion.md). How the
> `memory` source avoids ever learning or suggesting sensitive data (tokens,
> passwords, API keys), while still learning the command and option names.

## 1. Requirement

> The feature must be smart enough to skip / not suggest commands and options that
> contain sensitive data such as tokens, passwords, API keys, … The command and
> option are still suggested — just without the sensitive value.

So for `az login --password S3cr3t!` the engine should learn `az` (command) and
`--password` (option) but must **never** store or surface `S3cr3t!`. This applies
only to the `memory` source (the one built from what the user types); `manual` and
`external` are static catalogs authored/curated and are not redacted.

## 2. Design principle: redact at capture, never store the secret

Redaction happens **before** anything enters the in-RAM history ring
([01](01-architecture.md) §4). The store therefore never contains a secret, so
there is no way for a later suggestion, ranking, or debug dump to leak one. This is
strictly stronger than filtering at suggestion time.

```
raw command line ─▶ redact() ─▶ redacted line ─▶ CompletionHistory.record()
                       │
                       └─ secret values replaced/dropped; command + option names kept
```

`redact()` lives in `crates/completion/src/redact.rs` (gpui-free, heavily unit
tested). It composes with the existing `oneterm_terminal::security_policy`
helpers (`strip_unsafe_chars`, `truncate_utf8`) for control-char/length hygiene, so
this feature does not reinvent sanitization — it adds the *secret-value* layer on
top.

## 3. What "redact" does to a command line

Given a tokenized command line, redaction keeps structure but removes secret
**values**:

1. **Keep** the command name (`head`) and every **option flag name** (`--password`,
   `-p`, `/PASS`).
2. **Drop the value** that follows a secret-bearing option, replacing it with a
   placeholder so the option is still learnable:
   - `az login --password S3cr3t!`  →  `az login --password`
   - `curl -H "Authorization: Bearer abc.def"`  →  `curl -H` (the whole header
     value is dropped; the `-H` flag is kept)
3. **Drop `KEY=VALUE`** environment-style tokens whose key looks secret:
   - `AWS_SECRET_ACCESS_KEY=… aws s3 ls`  →  `aws s3 ls` (secret assignment removed
     entirely; the command survives)
4. **Drop standalone high-entropy / pattern-matching tokens** that look like
   credentials even without a telltale flag (see §4.3), replacing them with nothing
   so they are neither stored nor suggested.
5. If, after redaction, the line is *only* a command + options with no secrets, it
   is stored verbatim (the normal case).

The result is a line that is safe to store and safe to suggest, and that still
teaches the engine the command + its option flags.

## 4. Detection heuristics

Detection is layered; any layer triggering marks the following value (or the token
itself) as secret. Heuristics are conservative-but-practical: the cost of a false
positive is only "we didn't learn one argument value", which is acceptable, so we
err toward redacting.

### 4.1 Secret-bearing option names

A curated, case-insensitive list of flags whose argument is a secret. The value
after such a flag (whether `--flag value` or `--flag=value`) is dropped:

```
password, passwd, pwd, pass, secret, token, api-key, apikey, api_key,
access-key, secret-key, auth, authorization, bearer, credential, private-key,
client-secret, session-token, otp, passphrase, /PASSWORD, /P (context-limited)
```

Both `--flag value` and `--flag=value` (and Windows `/FLAG:value`) forms are
handled.

### 4.2 Secret-looking assignment keys

`KEY=VALUE` tokens where `KEY` matches the same secret vocabulary (or ends in
`_TOKEN` / `_KEY` / `_SECRET` / `_PASSWORD`) are dropped whole. Common env-var
prefixes (`AWS_`, `GITHUB_`, `OPENAI_`, `AZURE_`, …) combined with those suffixes
are strong signals.

### 4.3 Value shape / pattern detection

Standalone tokens (no telltale flag) are checked against well-known credential
shapes and an entropy threshold:

- **Known patterns:** AWS access key IDs (`AKIA…`), `ghp_…` / `github_pat_…`,
  `sk-…` (OpenAI-style), Slack `xox[baprs]-…`, JWTs (`eyJ…` three dot-separated
  base64url segments), `Bearer <token>`, connection strings with embedded
  credentials (`proto://user:pass@host`), private-key PEM markers.
- **High entropy:** long tokens (≥ ~20 chars) that are mostly base64/hex with
  high Shannon entropy and no dictionary structure are treated as probable
  secrets and dropped.
- URLs have their `user:pass@` userinfo stripped but the rest of the URL kept
  (the host/path is useful for suggestions and not secret).

### 4.4 Non-goals of detection

- It does not attempt perfect secret detection (an impossible task). It targets the
  common, high-value cases the requirement names (tokens, passwords, API keys).
- It does not scan program **output** — only the command **input** line captured
  for history. Output redaction is a separate concern handled elsewhere.

## 5. Defense in depth

Even though redaction happens at capture, a second, cheap guard runs at
**suggestion time**: before returning a history-derived suggestion, the engine
re-checks the candidate string for the §4 patterns and drops it if anything slips
through (e.g. a future code path that records without redacting). This makes it
very hard for a secret to reach the overlay regardless of capture-path bugs, and is
covered by tests that assert "a suggestion never contains a detected secret".

## 6. Configuration

- `completion.redact_sensitive` (default `true`, [06](06-configuration.md) §2)
  controls capture-time redaction. It should stay on; exposing it mainly documents
  the behavior and allows advanced users on trusted single-user machines to opt out
  knowingly. The suggestion-time guard (§5) always runs regardless.
- The secret-flag vocabulary and pattern set live in the engine as constants;
  Phase 2 can make them user-extensible via the manual catalog file.

## 7. Testing

`crates/completion` unit tests assert, at minimum:

- `az login --password X` → history stores `az login --password`, never `X`.
- `--password=X`, `/PASSWORD:X`, `-p X` forms all redact the value.
- `AWS_SECRET_ACCESS_KEY=… cmd` → stores `cmd`.
- A bare `ghp_…` / `sk-…` / JWT / `AKIA…` token in a command is never stored or
  suggested.
- `proto://user:pass@host/path` → stores `proto://host/path`.
- A normal command with no secrets is stored verbatim (no over-redaction of, e.g.,
  `--output file.txt`).
- Suggestion-time guard: injecting a raw secret into the ring (bypassing capture)
  still yields no secret-bearing suggestion.
