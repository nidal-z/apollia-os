# crates/apollia-desktop/AGENTS.md

> Local rules for the Rust half of the desktop application. Read after the root
> `AGENTS.md` and before editing this crate. The frontend under
> `crates/apollia-desktop/ui/` has its own rulebook,
> `crates/apollia-desktop/ui/AGENTS.md`, and this file does not repeat it.

This crate is the Tauri v2 shell: it boots an embedded runtime in-process,
exposes it to the webview as Tauri commands, bridges `RuntimeEvent` onto the
DOM, and owns the tray, the bundled agents and the STT capture. It is 30 942
lines under `src/`, the largest subtree in the workspace after
`apollia-runtime`, and its patterns are not the runtime's: the contract it has
to keep is with a webview, not with an HTTP client.

---

## 1. A command is a contract with the webview, in both directions

`#[tauri::command]` declares a handler; `tauri::generate_handler![...]` in
`src/main.rs` is what the application actually exposes. The two figures differ
whenever a command is `#[cfg]`-gated per target, so read the registration list,
not the attribute count.

Two guards hold the crossing, and both exist because it broke:

- `scripts/check_tauri_ipc_callers.py` refuses a registered command no
  `invoke(...)` reaches, and an `invoke(...)` naming a command nothing
  registers. Sixty-two commands once sat registered with no caller at all.
- `scripts/check_tauri_ipc_args.py` refuses an `invoke` whose argument names do
  not match the handler's parameters. Tauri serializes by name, so a typo is a
  runtime error the compiler cannot see and a test rarely covers.

Adding a command means: the Rust handler, the entry in `generate_handler!`, the
typed wrapper under `ui/src/lib/ipc/`, and a caller. A command without a caller
is dead surface, and the answer is to delete it rather than to add a caller.

---

## 2. An event needs a listener

`src/events.rs` forwards `RuntimeEvent` to the webview and gives each variant a
category through `categorize`. A variant in a category the interface does not
read reaches the webview and is dropped: no compiler, no test and no type sees
it. Give the variant a category the interface already reads, or add the
listener in the same commit.

The same rule applies to the custom DOM events the front emits to itself:
`scripts/check_custom_event_listeners.py` crosses every emitter against its
listeners.

---

## 3. The webview's CSP is a second belt, not the first

`tauri.conf.json` carries the production CSP, and
`scripts/check_desktop_csp.py` holds it to that role: `script-src 'self'` with
no `unsafe-inline` and no remote origin, `object-src 'none'`, `frame-src
'none'`, `connect-src` limited to `'self'` and the IPC origins. The first belt
is that the bundle carries no remote asset at all
(`scripts/check_no_font_cdn.py`); the CSP is what catches the one that slips
in.

Widening the CSP is a security decision and goes in the decisions chapter of
`docs/site/` before the code.

---

## 4. A child process shows no console window

Every process this crate spawns asks for no console window. On Windows a plain
`Command::spawn` flashes a black rectangle over the application for every
sidecar, every `python`, every `llama-server`, and
`scripts/check_subprocess_window.py` refuses a spawn site that does not go
through the shared helper.

The same guard is the reason a spawn is written once and reused: the flag is a
platform detail, and copying the site is how one copy loses it.

---

## 5. The runtime is embedded, not called over HTTP

`src/backend/` starts the actor mesh in-process. The desktop therefore holds
the same objects the daemon holds, and the temptation is to reach into them
from a command handler. Do not: a command calls the same handle the HTTP route
would call, so the two surfaces cannot drift.

A command that needs a capability the runtime does not expose gets the runtime
side first, exactly as the CLI does.

---

## 6. Forbidden in this crate

- A registered command with no caller, and an `invoke` with no command.
- A spawn that does not go through the no-console-window helper.
- Widening the production CSP without a decision recorded in `docs/site/`.
- Reaching into an actor's private state instead of sending it a message.
- `unwrap()`, `expect()`, `panic!()` outside tests. A panic here takes the
  whole window down, and the user sees a closed application with no message.

---

## 7. When the rules block you

- The webview needs data no command returns : add the command and its caller in
  the same commit, or the guard will report the half you shipped.
- A long operation blocks the UI : it belongs on the runtime side, emitted back
  as events, not awaited inside a command.
- A test needs the real window : there is no WebDriver for WKWebView on macOS.
  The gestural harness under `scripts/automation/` is what drives the real app;
  read its README before touching a `data-testid` a script names.
