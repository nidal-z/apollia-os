#!/usr/bin/env python3
"""Cross every desktop `invoke` call against the Tauri command it calls.

Tauri v2 converts the name of each Rust command argument to lowerCamelCase
before looking it up in the JavaScript argument object, and never falls back to
the snake_case spelling. A key the runtime does not find is not a default
value: the call is rejected, every single time, with `command <c> missing
required key <k>`. A surface built on such a call is shipped and dead.

This guard reads the two sides of that contract and compares them.

  * Rust side, `crates/apollia-desktop/src`: every `#[tauri::command]`, the
    arguments it declares, minus the ones Tauri injects itself, and whether
    each is optional.
  * JavaScript side, `crates/apollia-desktop/ui/src`: every `invoke` call that
    carries an object literal of arguments, and the keys of that literal.

Verdict by exit code, since the caller reads it rather than the text:

  0  every readable call carries the keys Tauri requires
  1  at least one readable call omits a required key
  2  nothing was measured, so the run says nothing about the tree

Calls this guard cannot read are counted and named on the same line as the
number of faults, so that a zero obtained by ceasing to measure is visible.
A call is unreadable when its command name is not a string literal, when its
argument object is spread or computed, or when the command it names is not
declared under `crates/apollia-desktop/src`.

Run it from anywhere; the two subtrees are resolved from this file's location.
Run it with `--selftest` to check the guard itself against a built subject.
"""

import argparse
import contextlib
import io
import re
import shutil
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
RUST_SUBTREE = Path("crates/apollia-desktop/src")
UI_SUBTREE = Path("crates/apollia-desktop/ui/src")

# Argument types Tauri injects itself. They never appear in the JavaScript
# argument object, so they impose no key. The list is exactly what the tree
# declares today; an injected type absent from it makes the guard report a
# missing key, which is the loud failure a guard should have.
INJECTED_TYPES = ("State", "tauri::State", "AppHandle", "tauri::AppHandle")

COMMAND_ATTRIBUTE = "#[tauri::command]"
FN_SIGNATURE = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*\(")
INVOKE_CALL = re.compile(r"\binvoke\s*(?:<[^;]*?>)?\s*\(")
STRING_LITERAL = re.compile(r"""^\s*(["'])([A-Za-z_]\w*)\1\s*""")
OBJECT_KEY = re.compile(r"""^(?:(["'])(\w+)\1|(\w+))\s*:""")
UI_SUFFIXES = (".ts", ".svelte")


def to_lower_camel(name: str) -> str:
    """Reproduce the `to_lower_camel_case` Tauri applies to argument names."""
    head, *rest = name.split("_")
    return head + "".join(part[:1].upper() + part[1:] for part in rest)


def _split_top_level(text: str, separator: str = ",") -> list[str]:
    parts, depth, current = [], 0, ""
    for char in text:
        if char in "<([{":
            depth += 1
        elif char in ">)]}":
            depth -= 1
        if char == separator and depth == 0:
            parts.append(current)
            current = ""
        else:
            current += char
    if current.strip():
        parts.append(current)
    return parts


def _balanced_slice(
    text: str, start: int, opener: str, closer: str, quotes: str = "\"'`"
) -> str | None:
    """Return the text between `start` and the matching closer, or None.

    `quotes` is empty on the Rust side: a lone `'` there opens a lifetime, not
    a string, and reading `State<'_, RuntimeHandle>` as a string swallows the
    rest of the signature.
    """
    depth, index = 0, start
    in_string: str | None = None
    while index < len(text):
        char = text[index]
        if in_string:
            if char == "\\":
                index += 2
                continue
            if char == in_string:
                in_string = None
        elif char in quotes:
            in_string = char
        elif char == opener:
            depth += 1
        elif char == closer:
            depth -= 1
            if depth == 0:
                return text[start + 1 : index]
        index += 1
    return None


def declared_commands(rust_root: Path) -> dict[str, dict[str, list[str]]]:
    """Map each `#[tauri::command]` to the argument keys it makes Tauri read."""
    commands: dict[str, dict[str, list[str]]] = {}
    for path in sorted(rust_root.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            if line.strip() != COMMAND_ATTRIBUTE:
                continue
            cursor = index + 1
            # Attributes, doc comments, plain comments and blank lines may sit
            # between the attribute and the signature. Three commands of this
            # tree carry a comment there.
            while cursor < len(lines):
                stripped = lines[cursor].strip()
                if FN_SIGNATURE.match(stripped):
                    break
                if stripped and not stripped.startswith(("#[", "//", "/*", "*")):
                    break
                cursor += 1
            if cursor >= len(lines):
                continue
            match = FN_SIGNATURE.match(lines[cursor].strip())
            if not match:
                continue
            block = "\n".join(lines[cursor : cursor + 40])
            signature = _balanced_slice(block, block.index("("), "(", ")", quotes="")
            if signature is None:
                continue
            required, optional = [], []
            for argument in _split_top_level(signature):
                argument = argument.strip()
                if not argument or ":" not in argument:
                    continue
                name, declared_type = argument.split(":", 1)
                name, declared_type = name.strip(), declared_type.strip()
                if declared_type.split("<")[0].strip() in INJECTED_TYPES:
                    continue
                key = to_lower_camel(name.lstrip("_"))
                if declared_type.startswith("Option<"):
                    optional.append(key)
                else:
                    required.append(key)
            commands[match.group(1)] = {
                "required": required,
                "optional": optional,
                "site": f"{path.relative_to(rust_root)}:{cursor + 1}",
            }
    return commands


def _is_comment(line: str) -> bool:
    stripped = line.lstrip()
    return stripped.startswith(("//", "*", "/*"))


def invoke_calls(ui_root: Path) -> list[dict[str, object]]:
    """Collect every `invoke` call that carries an object literal of arguments."""
    calls: list[dict[str, object]] = []
    for path in sorted(ui_root.rglob("*")):
        if path.suffix not in UI_SUFFIXES or not path.is_file():
            continue
        source = path.read_text(encoding="utf-8")
        lines = source.splitlines(keepends=True)
        offsets, running = [], 0
        for line in lines:
            offsets.append(running)
            running += len(line)
        for match in INVOKE_CALL.finditer(source):
            open_paren = match.end() - 1
            line_number = sum(1 for offset in offsets if offset <= match.start())
            if _is_comment(lines[line_number - 1]):
                continue
            arguments = _balanced_slice(source, open_paren, "(", ")")
            if arguments is None:
                continue
            site = f"{path.relative_to(ui_root)}:{line_number}"
            name_match = STRING_LITERAL.match(arguments)
            rest = arguments[name_match.end() :].lstrip() if name_match else ""
            if not rest.startswith(","):
                continue  # no argument object: nothing to cross
            payload = rest[1:].strip()
            if not payload:
                continue  # trailing comma after the command name, no arguments
            if not name_match:
                calls.append({"site": site, "command": None, "keys": None})
                continue
            command = name_match.group(2)
            if not payload.startswith("{"):
                calls.append({"site": site, "command": command, "keys": None})
                continue
            body = _balanced_slice(payload, 0, "{", "}")
            if body is None or "..." in body:
                calls.append({"site": site, "command": command, "keys": None})
                continue
            keys = []
            readable = True
            for entry in _split_top_level(body):
                entry = entry.strip()
                if not entry or entry.startswith("//"):
                    continue
                key_match = OBJECT_KEY.match(entry)
                if key_match:
                    keys.append(key_match.group(2) or key_match.group(3))
                elif re.fullmatch(r"\w+", entry):
                    keys.append(entry)  # shorthand property
                else:
                    readable = False
            calls.append(
                {"site": site, "command": command, "keys": keys if readable else None}
            )
    return calls


def cross(
    commands: dict[str, dict[str, list[str]]], calls: list[dict[str, object]]
) -> tuple[list[dict[str, object]], int]:
    """Return the faulty calls, and the count of the ones left unread."""
    faults, unreadable = [], 0
    for call in calls:
        command = call["command"]
        keys = call["keys"]
        if keys is None or command not in commands:
            unreadable += 1
            continue
        missing = [key for key in commands[command]["required"] if key not in keys]
        if missing:
            faults.append({**call, "missing": missing})
    return faults, unreadable


def report(rust_root: Path, ui_root: Path) -> int:
    commands = declared_commands(rust_root)
    calls = invoke_calls(ui_root)
    faults, unreadable = cross(commands, calls)
    print(f"tauri commands declared        : {len(commands)}")
    print(
        f"invoke calls with an argument object : {len(calls)} "
        f"(not readable by this guard: {unreadable})"
    )
    print(f"calls missing a required key   : {len(faults)}")
    for fault in faults:
        print(f"  {fault['site']}  {fault['command']}")
        print(f"      missing : {fault['missing']}")
        print(f"      passed  : {fault['keys']}")
    print()
    if not commands or not calls:
        print("NOTHING MEASURED: no command or no invoke call was found.")
        return 2
    if faults:
        print(
            f"FAIL: {len(faults)} invoke call(s) omit a key Tauri requires. "
            "Each one is rejected at every execution."
        )
        return 1
    print("OK: every readable invoke call carries the keys Tauri requires")
    return 0


# ── self-test ────────────────────────────────────────────────────────────────


def _write(root: Path, relative: str, body: str) -> None:
    target = root / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(body, encoding="utf-8")


def _build_subject(root: Path) -> tuple[Path, Path]:
    rust_root, ui_root = root / "rust", root / "ui"
    _write(
        rust_root,
        "commands/chat.rs",
        "/// Updates session configuration.\n"
        "#[tauri::command]\n"
        "pub async fn update_chat_session(\n"
        "    state: State<'_, RuntimeHandle>,\n"
        "    session_id: String,\n"
        "    update: UpdateSessionRequest,\n"
        ") -> Result<(), String> {\n"
        "    Ok(())\n"
        "}\n",
    )
    _write(
        rust_root,
        "commands/mcp_coaching.rs",
        "#[tauri::command]\n"
        "pub fn meta_generate_capabilities_coaching(\n"
        "    request: CoachingRequest,\n"
        ") -> Result<Vec<CoachingExample>, String> {\n"
        "    Ok(vec![])\n"
        "}\n",
    )
    _write(
        rust_root,
        "commands/packages.rs",
        "#[tauri::command]\n"
        "// The install runs on a blocking pool.\n"
        "// It can take minutes on a cold cache.\n"
        "#[allow(clippy::too_many_arguments)]\n"
        "pub async fn install_agent_package(\n"
        "    app: tauri::AppHandle,\n"
        "    package_id: String,\n"
        "    limit: Option<u32>,\n"
        ") -> Result<(), String> {\n"
        "    Ok(())\n"
        "}\n",
    )
    return rust_root, ui_root


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


def selftest() -> int:
    print("tauri ipc argument crossing: both directions on a built subject")
    root = Path(tempfile.mkdtemp(prefix="check-tauri-ipc-"))
    try:
        rust_root, ui_root = _build_subject(root)
        commands = declared_commands(rust_root)

        def measure(body: str) -> tuple[list[dict[str, object]], int]:
            """Cross the built commands against one JavaScript file, alone."""
            shutil.rmtree(ui_root, ignore_errors=True)
            _write(ui_root, "lib/subject.ts", body)
            return cross(commands, invoke_calls(ui_root))

        faults, _ = measure(
            'await invoke("update_chat_session", {\n'
            "  session_id: id,\n"
            "  update: { system_prompt: text },\n"
            "});\n"
            'examples = await invoke<Example[]>("meta_generate_capabilities_coaching", {\n'
            "  serverName,\n"
            "  serverTitle,\n"
            "});\n"
        )
        results = [
            _case("the three declared commands are all found", len(commands) == 3),
            _case(
                "a comment between the attribute and the signature is skipped",
                "install_agent_package" in commands,
            ),
            _case("the two known defect shapes are both found", len(faults) == 2),
            _case(
                "the missing key is named, not just the call",
                sorted(key for fault in faults for key in fault["missing"])
                == ["request", "sessionId"],
            ),
            _case(
                "each fault names the line that carries it",
                sorted(str(fault["site"]) for fault in faults)
                == ["lib/subject.ts:1", "lib/subject.ts:5"],
            ),
        ]

        faults, _ = measure(
            'await invoke("update_chat_session", {\n'
            "  sessionId: id,\n"
            "  update: { system_prompt: text },\n"
            "});\n"
            'examples = await invoke<Example[]>("meta_generate_capabilities_coaching", {\n'
            "  request: { serverName, serverTitle },\n"
            "});\n"
        )
        results.append(
            _case("positive control: the corrected forms raise nothing", not faults)
        )

        faults, _ = measure('await invoke("install_agent_package", { packageId: id });\n')
        results.append(_case("an Option argument is not required", not faults))

        faults, unreadable = measure(
            'await invoke("install_agent_package", { ...payload });\n'
            'await invoke("a_command_of_another_crate", { packageId: id });\n'
            'await invoke("install_agent_package", cond ? { packageId: id } : undefined);\n'
            '// await invoke("update_chat_session", { session_id: id });\n'
        )
        results.append(
            _case(
                "a spread, a conditional payload and a foreign command are counted, not judged",
                not faults and unreadable == 3,
            )
        )

        # The tree writes argument-less calls with the command name on its own
        # line and a trailing comma. Read as an argument object, such a call
        # inflates the unreadable count with something that carries no
        # arguments at all, which is exactly a zero obtained by not measuring.
        faults, unreadable = measure(
            "const list = await invoke<Session[]>(\n"
            '  "update_chat_session",\n'
            ");\n"
        )
        results.append(
            _case(
                "a trailing comma after the command name is not an argument object",
                not faults and unreadable == 0,
            )
        )

        shutil.rmtree(ui_root, ignore_errors=True)
        ui_root.mkdir(parents=True)
        with contextlib.redirect_stdout(io.StringIO()):
            empty_verdict = report(rust_root, ui_root)
        results.append(
            _case("a subject with no call measures nothing", empty_verdict == 2)
        )

        print()
        if all(results):
            print(f"self-test: all {len(results)} cases pass")
            return 0
        print(f"self-test: {results.count(False)} of {len(results)} cases fail")
        return 1
    finally:
        shutil.rmtree(root, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest", action="store_true", help="replay the fixture controls instead of measuring the tree"
    )
    if parser.parse_args().selftest:
        sys.exit(selftest())
    rust_root, ui_root = REPO_ROOT / RUST_SUBTREE, REPO_ROOT / UI_SUBTREE
    for subtree in (rust_root, ui_root):
        if not subtree.is_dir():
            print(f"NOTHING MEASURED: {subtree} is absent from this tree.")
            sys.exit(2)
    sys.exit(report(rust_root, ui_root))


if __name__ == "__main__":
    main()
