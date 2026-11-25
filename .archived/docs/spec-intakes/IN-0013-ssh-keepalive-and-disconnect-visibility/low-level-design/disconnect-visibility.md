# Low-Level Design: disconnect-visibility

Intake: IN-0013
HLD: high-level-design.md
Topic: disconnect-visibility
Date: 2026-08-21

## Concern

Make an SSH `SessionEvent::Closed` visible and persistent in terminal-view while preserving safe read-only access to the rendered terminal history.

## Design

- Keep the existing public `SessionEvent::Closed` shape; the SSH task currently has only teardown labels, not a durable user-facing reason contract.
- Add a terminal-view closed-session marker set only when `Closed` arrives for `SessionKind::Ssh`.
- On the first transition:
  - mark the view not alive so cursor blinking and completion stop;
  - queue one error notification for render-time delivery;
  - retain the terminal model and view instead of shutting down/closing the tab.
- Render a non-dismissible bottom banner above terminal content: `SSH connection closed. Input is disabled.`
- Gate outbound input using `TerminalLifecycle::alive()` at the shared write boundary/handlers while retaining selection, copy, scroll, search, and prior output rendering.
- Repeated `Closed` events are idempotent and do not enqueue repeated toasts.
- Local process exit/close behavior remains unchanged.

## Interfaces

No terminal/backend public event shape changes. Terminal-view owns one optional closed-state value and one pure banner-construction/helper seam suitable for tests.

## Edge Cases and Failure Modes

- [x] A remote close after an exit status still produces one SSH closed presentation.
- [x] Closing/removing the tab after remote closure remains idempotent.
- [x] A full OSC notification queue cannot silently discard the lifecycle toast; the view records the closed marker independently and the banner remains visible.
- [x] Input arriving between transport teardown and event delivery still fails safely at the existing transport boundary.
- [x] Search, selection, copy, and scrolling continue to operate on prior content.

## Verification

- [x] A focused view test folds SSH closure into read-only state exactly once.
- [x] Render/helper proof checks the persistent banner is present only after SSH closure.
- [x] Input regression verifies no outbound write reaches the transport after the session is no longer alive; existing non-write action tests remain green.
- [x] Existing terminal-view lifecycle and panel shutdown tests remain green.
