#!/usr/bin/env python3
"""Hold the desktop webview's production CSP to its second-belt role.

The webview renders content written by the model and quoted from the web.
DOMPurify is the first belt; the CSP is the second, and `script-src
'unsafe-inline'` removes it exactly when it matters, when a sanitization
fails. The directive was widened for one inline theme script that Tauri
hashes on its own at build time, so the width bought nothing and cost the
belt. `withGlobalTauri` exposes `window.__TAURI__` to every script of the
page while no source file reads it.

What this holds, on `crates/apollia-desktop/tauri.conf.json`:

  - `script-src` carries neither `'unsafe-inline'` nor `'unsafe-eval'`;
  - no directive of the production CSP carries `'unsafe-eval'`;
  - no source of the production CSP names an external host: `'self'`,
    scheme-only sources (`data:`, `blob:`, `ipc:`) and the loopback hosts
    Tauri itself serves (`ipc.localhost`, `localhost`, `127.0.0.1`) are the
    whole allowance;
  - `app.withGlobalTauri` is absent or false.

`devCsp` is deliberately out of scope: it never ships (it applies to the
`devUrl` webview only) and Vite's dev server needs the inline and eval
allowances the production CSP must not have. `style-src 'unsafe-inline'`
stays allowed: Svelte writes `style` attributes at runtime, which no
build-time hash can cover, and the constat this guard closes is about
scripts.

Exit codes: 0 when the file holds the form, 1 when a defect is found,
2 when the config file is missing (nothing measured, which is not a pass).

Usage:
    python3 scripts/check_desktop_csp.py
    python3 scripts/check_desktop_csp.py --selftest
"""

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
CONF = Path("crates/apollia-desktop/tauri.conf.json")

ALLOWED_HOSTS = {"ipc.localhost", "localhost", "127.0.0.1"}


def _host_of(source: str) -> str | None:
    """The host part of a CSP source, or None for host-less sources."""
    if "://" not in source:
        return None
    rest = source.split("://", 1)[1]
    return rest.split("/", 1)[0].split(":", 1)[0].lower()


def csp_offenses(csp: str) -> list[str]:
    """Every way the production CSP string breaks the form, one line each.

    Pure, so the selftest can drive it on both a violating and a clean
    sample: a detector only ever run against a clean tree is a detector
    nobody has tested.
    """
    offenses: list[str] = []
    for directive in csp.split(";"):
        parts = directive.strip().split()
        if not parts:
            continue
        name, sources = parts[0], parts[1:]
        if name == "script-src" and "'unsafe-inline'" in sources:
            offenses.append(
                "script-src carries 'unsafe-inline': the CSP no longer backs "
                "DOMPurify when a sanitization fails (Tauri hashes the bundled "
                "inline scripts itself, so nothing needs the width)"
            )
        if "'unsafe-eval'" in sources:
            offenses.append(f"{name} carries 'unsafe-eval'")
        for source in sources:
            host = _host_of(source)
            if host is not None and host not in ALLOWED_HOSTS:
                offenses.append(
                    f"{name} names the external host {source}: the production "
                    f"webview must not be told to trust anything remote"
                )
    return offenses


def conf_offenses(conf: dict) -> list[str]:
    """The offenses of a parsed tauri.conf.json."""
    app = conf.get("app", {})
    offenses = []
    if app.get("withGlobalTauri", False):
        offenses.append(
            "app.withGlobalTauri is true: window.__TAURI__ is handed to every "
            "script of the page, and no source file reads it"
        )
    csp = app.get("security", {}).get("csp")
    if not isinstance(csp, str) or not csp.strip():
        offenses.append(
            "app.security.csp is missing or empty: the webview would run with "
            "no production CSP at all"
        )
        return offenses
    return offenses + csp_offenses(csp)


def selftest() -> int:
    """Both directions, on fixtures rather than on the tree."""
    bad = {
        "app": {
            "withGlobalTauri": True,
            "security": {
                "csp": "default-src 'self'; script-src 'self' 'unsafe-inline'; "
                "style-src 'self' 'unsafe-eval'; "
                "connect-src 'self' https://api.example.com"
            },
        }
    }
    found = conf_offenses(bad)
    if len(found) != 4:
        print(
            f"selftest: the violating fixture must yield 4 offenses "
            f"(withGlobalTauri, inline, eval, external host), got "
            f"{len(found)}: {found}",
            file=sys.stderr,
        )
        return 1
    good = {
        "app": {
            "security": {
                "csp": "default-src 'self'; script-src 'self'; "
                "style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; "
                "connect-src 'self' ipc: http://ipc.localhost"
            }
        }
    }
    found = conf_offenses(good)
    if found:
        print(
            f"selftest: the clean fixture must yield no offense, got: {found}",
            file=sys.stderr,
        )
        return 1
    missing = conf_offenses({})
    if len(missing) != 1 or "missing or empty" not in missing[0]:
        print(
            f"selftest: a conf without a csp must yield the no-CSP offense, "
            f"got: {missing}",
            file=sys.stderr,
        )
        return 1
    print("check_desktop_csp: selftest, both directions hold")
    return 0


def main() -> int:
    path = REPO_ROOT / CONF
    if not path.is_file():
        print(
            f"check_desktop_csp: NOTHING MEASURED, {CONF} does not exist",
            file=sys.stderr,
        )
        return 2
    try:
        conf = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"check_desktop_csp: NOTHING MEASURED, cannot read {CONF}: {exc}", file=sys.stderr)
        return 2

    offenses = conf_offenses(conf)
    csp = conf.get("app", {}).get("security", {}).get("csp", "")
    directives = sum(1 for d in csp.split(";") if d.strip())
    print(f"check_desktop_csp: {directives} production directives read from {CONF}")
    if offenses:
        print(f"\n{len(offenses)} offense(s):", file=sys.stderr)
        for offense in offenses:
            print(f"  {offense}", file=sys.stderr)
        return 1
    print("check_desktop_csp: the production CSP holds its form")
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest",
        action="store_true",
        help="drive the detector on a violating and a clean fixture, then exit",
    )
    args = parser.parse_args()
    sys.exit(selftest() if args.selftest else main())
