#!/usr/bin/env python3
"""Cross every registered Tauri command against the calls that reach it.

A command registered in `generate_handler!` and called by nothing is a surface
that ships, compiles, passes its own unit tests, and is reachable by no user.
Nineteen of them lived in this tree at once. The sibling guard,
`scripts/check_tauri_ipc_args.py`, already reads the two sides of the IPC
contract; it asks whether a call carries the keys the command reads. This one
asks whether a command is called at all, and it reads a third side the sibling
does not need.

  * definitions, `crates/apollia-desktop/src`: every `#[tauri::command]`, kept
    per site rather than per name, because one name may be defined twice under
    opposite `#[cfg]` and a set would hide it.
  * registrations, the `generate_handler!` block(s) of the same subtree: the
    entries the webview can actually reach, plus the compilation conditions
    that gate some of them.
  * calls, `crates/apollia-desktop/ui/src`: every `invoke` call, whether its
    argument object is readable or not, which is where this guard parts company
    with its sibling.

The crossing carries two ranks. The first proves every command has a call
site. The second reads the wrapper layer itself, because a wrapper exported
from `ui/src/lib/ipc/` counted as a caller even when nothing imported it,
which is how `list_projects_for_agent` shipped reachable by no user: no
`invoke` in a `.svelte` file, no exported wrapper without a caller (a helper
carrying no `invoke` may live for its test alone), no command whose every
literal site sits in a dead wrapper, and a wrapper naming exactly one command
is called `camelCase(command)`.

Verdict by exit code, since the caller reads it rather than the text:

  0  every registered command has a caller, every invoke names a registered
     command, the two completeness lines hold, and the wrapper layer holds
     the four rules above
  1  at least one registered command has no caller, or an invoke names a
     command that is not registered, or a line was left uninterpreted, or the
     registered / defined mismatch is not empty, or a second-rank rule is
     broken
  2  nothing was measured, so the run says nothing about the tree: one of the
     two subtrees is absent, or no registration block, or no command, or no
     invoke call was found

The two completeness lines are the point. A `commands without a caller` list
that is empty proves nothing on its own: it is exactly what a guard that
stopped reading one of its three sides would print. So the number of lines left
uninterpreted must be zero, and the registered / defined mismatch must be
empty, or the verdict is 1 whatever the crossing found.

An `invoke` whose first argument is an identifier rather than a literal is not
counted as unreadable when the file itself declares the commands it may reach:
a literal collection of strings assigned to a constant, and a condition
matching the identifier against it through `.has(` or `.includes(`, both placed
before the call. The members of that collection are then credited as callers,
and the site is printed with the names it resolved. Failing that, the site is a
line left uninterpreted and the verdict is 1: an unguarded computed call lets
the webview choose the command freely, which is worth a red on its own.

Run it from anywhere; the two subtrees are resolved from this file's location.
Run it with `--selftest` to check the guard itself against a built subject.
"""

import contextlib
import io
import re
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_tauri_ipc_args as ipc_args  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parent.parent
RUST_SUBTREE = Path("crates/apollia-desktop/src")
UI_SUBTREE = Path("crates/apollia-desktop/ui/src")

HANDLER_OPEN = re.compile(r"generate_handler\s*!\s*\[")
HANDLER_ENTRY = re.compile(r"^(?:\w+\s*::\s*)*(\w+)$")
IDENTIFIER = re.compile(r"^\w+$")
LITERAL_COLLECTION = re.compile(
    r"\bconst\s+(\w+)\s*(?::[^=]*)?=\s*(?:new\s+(?:Set|Array)\s*\(\s*)?\["
)
STRING_ENTRY = re.compile(r"""^(["'`])([A-Za-z_]\w*)\1$""")


def _line_of(source: str, offset: int) -> int:
    return source.count("\n", 0, offset) + 1


# ── side one, the definitions ────────────────────────────────────────────────
# The sibling's `declared_commands` keys its result by name, which is right for
# its question and wrong for this one: `install_cli` is defined twice, under
# `#[cfg(unix)]` and `#[cfg(not(unix))]`, and a dictionary keeps one of the two.
# The walk below is the sibling's, kept per site.


def command_definitions(rust_root: Path) -> list[dict[str, str]]:
    """Every `#[tauri::command]` of the subtree, one entry per definition."""
    definitions: list[dict[str, str]] = []
    for path in sorted(rust_root.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            if line.strip() != ipc_args.COMMAND_ATTRIBUTE:
                continue
            cursor = index + 1
            while cursor < len(lines):
                stripped = lines[cursor].strip()
                if ipc_args.FN_SIGNATURE.match(stripped):
                    break
                if stripped and not stripped.startswith(("#[", "//", "/*", "*")):
                    break
                cursor += 1
            if cursor >= len(lines):
                continue
            match = ipc_args.FN_SIGNATURE.match(lines[cursor].strip())
            if not match:
                continue
            definitions.append(
                {
                    "name": match.group(1),
                    "site": f"{path.relative_to(rust_root)}:{cursor + 1}",
                }
            )
    return definitions


# ── side two, the registrations ──────────────────────────────────────────────


def registrations(rust_root: Path) -> tuple[list[dict[str, str]], list[str], int]:
    """Read every `generate_handler!` block: entries, unread lines, block count."""
    entries: list[dict[str, str]] = []
    unread: list[str] = []
    blocks = 0
    for path in sorted(rust_root.rglob("*.rs")):
        source = path.read_text(encoding="utf-8")
        for match in HANDLER_OPEN.finditer(source):
            open_bracket = match.end() - 1
            body = ipc_args._balanced_slice(source, open_bracket, "[", "]", quotes="")
            if body is None:
                unread.append(
                    f"{path.relative_to(rust_root)}:{_line_of(source, match.start())}"
                    "  generate_handler! block has no matching bracket"
                )
                continue
            blocks += 1
            first_line = _line_of(source, open_bracket)
            condition = ""
            for offset, raw in enumerate(body.splitlines()):
                stripped = raw.strip().rstrip(",").strip()
                site = f"{path.relative_to(rust_root)}:{first_line + offset}"
                if not stripped or stripped.startswith(("//", "/*", "*")):
                    continue
                if stripped.startswith("#["):
                    # An attribute inside the block gates the entry that
                    # follows it. Carried, never dropped: three commands of
                    # this tree are registered only under `debug_assertions`,
                    # and a reader who drops the line cannot see that.
                    condition = stripped
                    continue
                entry = HANDLER_ENTRY.match(stripped)
                if not entry:
                    unread.append(f"{site}  {stripped}")
                    condition = ""
                    continue
                entries.append(
                    {"name": entry.group(1), "site": site, "condition": condition}
                )
                condition = ""
    return entries, unread, blocks


# ── side three, the calls ────────────────────────────────────────────────────


def _literal_collections(source: str) -> dict[str, dict[str, object]]:
    """Constants bound to a literal collection of strings, with their offset."""
    found: dict[str, dict[str, object]] = {}
    for match in LITERAL_COLLECTION.finditer(source):
        body = ipc_args._balanced_slice(source, match.end() - 1, "[", "]")
        if body is None:
            continue
        members = []
        readable = True
        for entry in ipc_args._split_top_level(body):
            entry = entry.strip()
            if not entry:
                continue
            literal = STRING_ENTRY.match(entry)
            if not literal:
                readable = False
                break
            members.append(literal.group(2))
        if readable and members:
            found[match.group(1)] = {"members": members, "offset": match.start()}
    return found


def _resolve_through_declared_list(
    source: str, identifier: str, call_offset: int
) -> list[str] | None:
    """Names an identifier can carry, when a declared list guards the call."""
    for name, collection in _literal_collections(source).items():
        offset = collection["offset"]
        if not isinstance(offset, int) or offset >= call_offset:
            continue
        guard = re.compile(
            rf"\b{re.escape(name)}\s*\.\s*(?:has|includes)\s*\(\s*{re.escape(identifier)}\b"
        )
        match = guard.search(source)
        if match and match.start() < call_offset:
            members = collection["members"]
            return list(members) if isinstance(members, list) else None
    return None


def invoke_sites(ui_root: Path) -> list[dict[str, object]]:
    """Every `invoke` call of the subtree, resolved to the names it may reach."""
    sites: list[dict[str, object]] = []
    for path in sorted(ui_root.rglob("*")):
        if path.suffix not in ipc_args.UI_SUFFIXES or not path.is_file():
            continue
        source = path.read_text(encoding="utf-8")
        lines = source.splitlines(keepends=True)
        for match in ipc_args.INVOKE_CALL.finditer(source):
            line_number = _line_of(source, match.start())
            if ipc_args._is_comment(lines[line_number - 1]):
                continue
            arguments = ipc_args._balanced_slice(source, match.end() - 1, "(", ")")
            site = f"{path.relative_to(ui_root)}:{line_number}"
            if arguments is None:
                sites.append({"site": site, "names": None, "how": "unreadable"})
                continue
            literal = ipc_args.STRING_LITERAL.match(arguments)
            if literal:
                sites.append(
                    {"site": site, "names": [literal.group(2)], "how": "literal"}
                )
                continue
            first = (ipc_args._split_top_level(arguments) or [""])[0].strip()
            if IDENTIFIER.match(first):
                names = _resolve_through_declared_list(source, first, match.start())
                if names is not None:
                    sites.append({"site": site, "names": names, "how": "declared list"})
                    continue
            sites.append({"site": site, "names": None, "how": "unreadable"})
    return sites


# ── the second rank, the wrapper layer itself ────────────────────────────────
# The first rank proves every command has a call site. It stops at the literal:
# a wrapper exported from `lib/ipc/` counted as a caller even when nothing
# imported it, which is how `list_projects_for_agent` shipped reachable by no
# user. The second rank reads the wrapper layer against the rules of
# `crates/apollia-desktop/ui/AGENTS.md` section 6:
#
#   * no `invoke` in a `.svelte` file: components go through `lib/ipc/`;
#   * an exported wrapper that carries an `invoke` has a caller outside tests
#     (a helper without an `invoke` may live for its test alone, section 11
#     sanctions exporting the logic under test);
#   * a command whose every literal site sits in a dead wrapper is dead;
#   * a wrapper naming exactly one command is called `camelCase(command)`.

IPC_DIR = "lib/ipc"
EXPORT_DECL = re.compile(
    r"^export\s+(?:async\s+)?(?:function|const|let)\s+([A-Za-z_$][\w$]*)", re.M
)


def camel_case(command: str) -> str:
    parts = command.split("_")
    return parts[0] + "".join(part.capitalize() for part in parts[1:])


def _ui_sources(ui_root: Path) -> dict[Path, str]:
    """Every non-test source of the subtree, path -> text."""
    sources: dict[Path, str] = {}
    for path in sorted(ui_root.rglob("*")):
        if path.suffix not in ipc_args.UI_SUFFIXES or not path.is_file():
            continue
        if path.name.endswith(".test.ts"):
            continue
        sources[path] = path.read_text(encoding="utf-8")
    return sources


def ipc_wrappers(ui_root: Path) -> list[dict[str, object]]:
    """Exported symbols of `lib/ipc/*.ts` with the commands their span invokes."""
    wrappers: list[dict[str, object]] = []
    ipc_root = ui_root / IPC_DIR
    if not ipc_root.is_dir():
        return wrappers
    for path in sorted(ipc_root.glob("*.ts")):
        if path.name.endswith(".test.ts"):
            continue
        source = path.read_text(encoding="utf-8")
        spans = [(m.start(), m.group(1)) for m in EXPORT_DECL.finditer(source)]
        spans.append((len(source), None))
        for (start, name), (end, _next) in zip(spans, spans[1:]):
            if name is None:
                continue
            commands: set[str] = set()
            body = source[start:end]
            for match in ipc_args.INVOKE_CALL.finditer(body):
                arguments = ipc_args._balanced_slice(body, match.end() - 1, "(", ")")
                if arguments is None:
                    continue
                literal = ipc_args.STRING_LITERAL.match(arguments)
                if literal:
                    commands.add(literal.group(2))
            wrappers.append({"file": path, "name": name, "commands": commands})
    return wrappers


def second_rank(ui_root: Path, sites: list[dict[str, object]]) -> dict[str, list]:
    """The three wrapper-layer crossings, resolved against the whole subtree."""
    sources = _ui_sources(ui_root)
    tests = {
        path: path.read_text(encoding="utf-8")
        for path in sorted(ui_root.rglob("*.test.ts"))
        if path.is_file()
    }

    svelte_sites = [
        str(site["site"]) for site in sites if str(site["site"]).split(":")[0].endswith(".svelte")
    ]

    wrappers = ipc_wrappers(ui_root)
    dead: list[str] = []
    dead_spans: set[tuple[Path, str]] = set()
    misnamed: list[str] = []
    for wrapper in wrappers:
        path, name = wrapper["file"], str(wrapper["name"])
        commands = wrapper["commands"]
        if not isinstance(path, Path) or not isinstance(commands, set):
            continue
        reference = re.compile(r"\b" + re.escape(name) + r"\b")
        called_in_prod = any(
            reference.search(text) for other, text in sources.items() if other != path
        )
        called_in_test = any(reference.search(text) for text in tests.values())
        alive = called_in_prod or (called_in_test and not commands)
        if not alive:
            relative = path.relative_to(ui_root)
            dead.append(f"{relative}: {name} -> invoke {sorted(commands)}")
            dead_spans.add((path, name))
        if len(commands) == 1:
            (command,) = commands
            expected = camel_case(command)
            if name != expected:
                relative = path.relative_to(ui_root)
                misnamed.append(f"{relative}: {name} wraps {command}, expected {expected}")

    # A command is dead when its every literal site sits inside a dead wrapper.
    live_commands: set[str] = set()
    dead_commands_seen: dict[str, set[str]] = {}
    for path, source in sources.items():
        spans = [(m.start(), m.group(1)) for m in EXPORT_DECL.finditer(source)]
        for match in ipc_args.INVOKE_CALL.finditer(source):
            arguments = ipc_args._balanced_slice(source, match.end() - 1, "(", ")")
            if arguments is None:
                continue
            literal = ipc_args.STRING_LITERAL.match(arguments)
            if not literal:
                continue
            command = literal.group(2)
            enclosing = None
            for start, name in spans:
                if start <= match.start():
                    enclosing = name
            if enclosing is not None and (path, enclosing) in dead_spans:
                dead_commands_seen.setdefault(command, set()).add(str(enclosing))
            else:
                live_commands.add(command)
    dead_commands = [
        f"{command}  <- {', '.join(sorted(names))}"
        for command, names in sorted(dead_commands_seen.items())
        if command not in live_commands
    ]

    return {
        "svelte_sites": svelte_sites,
        "wrappers": wrappers,
        "dead_wrappers": dead,
        "dead_commands": dead_commands,
        "misnamed": misnamed,
    }


# ── the crossing ─────────────────────────────────────────────────────────────


def report(rust_root: Path, ui_root: Path) -> int:
    definitions = command_definitions(rust_root)
    entries, unread, blocks = registrations(rust_root)
    sites = invoke_sites(ui_root)

    defined_names = sorted({item["name"] for item in definitions})
    duplicated = sorted(
        {
            item["name"]
            for item in definitions
            if sum(1 for other in definitions if other["name"] == item["name"]) > 1
        }
    )
    registered_names = sorted({entry["name"] for entry in entries})
    conditioned = [entry for entry in entries if entry["condition"]]
    mismatch = sorted(set(registered_names) ^ set(defined_names))

    uninterpreted = list(unread)
    called: set[str] = set()
    resolved_sites = []
    literal_count = 0
    for site in sites:
        names = site["names"]
        if names is None:
            uninterpreted.append(f"{site['site']}  invoke with a name this guard "
                                 "cannot resolve")
            continue
        if site["how"] == "literal":
            literal_count += 1
        else:
            resolved_sites.append(f"{site['site']} -> {names}")
        called.update(name for name in names if isinstance(name, str))

    without_caller = sorted(set(registered_names) - called)
    absent = sorted(called - set(registered_names))

    print(f"registration blocks read      : {blocks}")
    print(
        f"commands defined              : {len(definitions)} "
        f"(distinct names {len(defined_names)}, defined more than once {duplicated})"
    )
    print(
        f"commands registered           : {len(registered_names)} "
        f"(under a compilation condition: {len(conditioned)})"
    )
    print(f"registered / defined mismatch : {mismatch}")
    print(f"lines left uninterpreted      : {len(uninterpreted)} {uninterpreted}")
    print(
        f"invoke calls read             : {len(sites)} "
        f"(literal name {literal_count}, resolved through a declared list "
        f"{len(resolved_sites)}: {', '.join(resolved_sites)})"
    )
    print(f"commands called               : {len(set(registered_names) & called)}")
    print(f"commands without a caller     : {without_caller}")
    print(f"invoke to an absent command   : {absent}")
    for name in absent:
        for site in sites:
            names = site["names"]
            if isinstance(names, list) and name in names:
                print(f"  {site['site']}  {name}")

    layer = second_rank(ui_root, sites)
    print(f"ipc wrappers read             : {len(layer['wrappers'])}")
    print(f"invoke sites in .svelte       : {len(layer['svelte_sites'])}")
    for site in layer["svelte_sites"]:
        print(f"  {site}")
    print(f"wrappers without a caller     : {len(layer['dead_wrappers'])}")
    for line in layer["dead_wrappers"]:
        print(f"  {line}")
    print(f"commands only behind dead wrappers : {len(layer['dead_commands'])}")
    for line in layer["dead_commands"]:
        print(f"  {line}")
    print(f"wrappers not named camelCase(command) : {len(layer['misnamed'])}")
    for line in layer["misnamed"]:
        print(f"  {line}")
    print()

    if not blocks or not definitions or not sites:
        print("NOTHING MEASURED: no registration block, no command or no invoke call.")
        return 2
    faults = []
    if without_caller:
        faults.append(f"{len(without_caller)} registered command(s) without a caller")
    if absent:
        faults.append(f"{len(absent)} invoke call(s) naming a command nothing registers")
    if uninterpreted:
        faults.append(f"{len(uninterpreted)} line(s) left uninterpreted")
    if mismatch:
        faults.append(f"{len(mismatch)} name(s) registered or defined but not both")
    if layer["svelte_sites"]:
        faults.append(
            f"{len(layer['svelte_sites'])} invoke call(s) in a .svelte file "
            f"instead of a lib/ipc wrapper"
        )
    if layer["dead_wrappers"]:
        faults.append(f"{len(layer['dead_wrappers'])} lib/ipc wrapper(s) without a caller")
    if layer["dead_commands"]:
        faults.append(
            f"{len(layer['dead_commands'])} command(s) reached only through a dead wrapper"
        )
    if layer["misnamed"]:
        faults.append(
            f"{len(layer['misnamed'])} wrapper(s) not named camelCase of their command"
        )
    if faults:
        print(f"FAIL: {'; '.join(faults)}.")
        return 1
    print("OK: every registered Tauri command has a caller, and the crossing is complete")
    return 0


# ── self-test ────────────────────────────────────────────────────────────────


def _write(root: Path, relative: str, body: str) -> None:
    target = root / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(body, encoding="utf-8")


def _build_subject(root: Path) -> tuple[Path, Path]:
    """A Rust side carrying every shape the real subtree carries."""
    rust_root, ui_root = root / "rust", root / "ui"
    _write(
        rust_root,
        "commands/agents.rs",
        "#[tauri::command]\n"
        "pub async fn list_agents() -> Result<Vec<Agent>, String> {\n"
        "    Ok(vec![])\n"
        "}\n"
        "\n"
        "/// Installs an agent from a package.\n"
        "#[tauri::command]\n"
        "pub async fn install_agent(id: String) -> Result<(), String> {\n"
        "    Ok(())\n"
        "}\n"
        "\n"
        "#[tauri::command]\n"
        "pub async fn create_trigger(id: String) -> Result<(), String> {\n"
        "    Ok(())\n"
        "}\n"
        "\n"
        "#[tauri::command]\n"
        "pub async fn get_agent_detail(id: String) -> Result<Agent, String> {\n"
        "    Ok(Agent::default())\n"
        "}\n",
    )
    _write(
        rust_root,
        "commands/cli.rs",
        "#[cfg(not(unix))]\n"
        "#[tauri::command]\n"
        "pub fn install_cli() -> Result<(), String> {\n"
        "    Ok(())\n"
        "}\n"
        "\n"
        "#[cfg(unix)]\n"
        "#[tauri::command]\n"
        "pub fn install_cli() -> Result<(), String> {\n"
        "    Ok(())\n"
        "}\n",
    )
    _write(
        rust_root,
        "main.rs",
        "fn main() {\n"
        "    tauri::Builder::default()\n"
        "        .invoke_handler(tauri::generate_handler![\n"
        "            commands::agents::list_agents,\n"
        "            commands::agents::install_agent,\n"
        "            commands::agents::create_trigger,\n"
        "            // Detail panel\n"
        "            commands::agents::get_agent_detail,\n"
        "            #[cfg(debug_assertions)]\n"
        "            commands::cli::install_cli,\n"
        "        ])\n"
        "        .run(tauri::generate_context!())\n"
        "        .expect(\"failed\");\n"
        "}\n",
    )
    return rust_root, ui_root


CALLS_EVERY_COMMAND = (
    'await invoke("list_agents");\n'
    'await invoke("install_agent", { id });\n'
    'await invoke("create_trigger", { id });\n'
    'await invoke("get_agent_detail", { id });\n'
    'await invoke("install_cli");\n'
)


def _case(name: str, condition: bool) -> bool:
    print(f"  {'ok  ' if condition else 'FAIL'}  {name}")
    return condition


def selftest() -> int:
    print("tauri command caller crossing: both directions on a built subject")
    root = Path(tempfile.mkdtemp(prefix="check-tauri-ipc-callers-"))
    try:
        rust_root, ui_root = _build_subject(root)

        def measure(body: str) -> tuple[str, int]:
            """Run the whole report against one JavaScript file, alone."""
            shutil.rmtree(ui_root, ignore_errors=True)
            _write(ui_root, "lib/subject.ts", body)
            buffer = io.StringIO()
            with contextlib.redirect_stdout(buffer):
                code = report(rust_root, ui_root)
            return buffer.getvalue(), code

        text, code = measure(
            'await invoke("list_agents");\n'
            'await invoke("install_agent", { id });\n'
            'await invoke("create_trigger", { id });\n'
            'await invoke("install_cli");\n'
        )
        results = [
            _case("a registered command nothing calls is a red", code == 1),
            _case(
                "and the guard names it",
                "commands without a caller     : ['get_agent_detail']" in text,
            ),
        ]

        text, code = measure(CALLS_EVERY_COMMAND)
        results.append(
            _case("positive control: the same tree, one call added, is green", code == 0)
        )
        results.append(
            _case(
                "a name defined twice under opposite cfg opens no mismatch",
                "registered / defined mismatch : []" in text
                and "defined more than once ['install_cli']" in text,
            )
        )
        results.append(
            _case(
                "an entry behind an attribute is registered, and the attribute counted",
                "(under a compilation condition: 1)" in text,
            )
        )
        results.append(
            _case(
                "a comment inside the registration block is not an uninterpreted line",
                "lines left uninterpreted      : 0 []" in text,
            )
        )

        text, code = measure(
            'const ALLOWED = new Set(["create_trigger", "install_agent"]);\n'
            'await invoke("list_agents");\n'
            'await invoke("get_agent_detail", { id });\n'
            'await invoke("install_cli");\n'
            "if (ALLOWED.has(command)) {\n"
            "  await invoke(command, args);\n"
            "}\n"
        )
        results.append(
            _case(
                "a computed call guarded by a declared list credits its members",
                code == 0 and "lib/subject.ts:6 -> ['create_trigger', 'install_agent']"
                in text,
            )
        )

        text, code = measure(
            CALLS_EVERY_COMMAND + "await invoke(command, args);\n",
        )
        results.append(
            _case(
                "the same call without a declared list is a line left uninterpreted",
                code == 1 and "lines left uninterpreted      : 1 " in text,
            )
        )

        text, code = measure(CALLS_EVERY_COMMAND + 'await invoke("no_such_command");\n')
        results.append(
            _case(
                "an invoke naming a command nothing registers is a red",
                code == 1
                and "invoke to an absent command   : ['no_such_command']" in text,
            )
        )

        # ── second rank: the wrapper layer ───────────────────────────────────

        def measure_files(files: dict[str, str]) -> tuple[str, int]:
            """Run the whole report against a built ui subtree."""
            shutil.rmtree(ui_root, ignore_errors=True)
            for relative, body in files.items():
                _write(ui_root, relative, body)
            buffer = io.StringIO()
            with contextlib.redirect_stdout(buffer):
                code = report(rust_root, ui_root)
            return buffer.getvalue(), code

        WRAPPED_EVERY_COMMAND = (
            'export function listAgents() { return invoke("list_agents"); }\n'
            'export function installAgent(id) { return invoke("install_agent", { id }); }\n'
            'export function createTrigger(id) { return invoke("create_trigger", { id }); }\n'
            'export function getAgentDetail(id) { return invoke("get_agent_detail", { id }); }\n'
            'export function installCli() { return invoke("install_cli"); }\n'
        )
        CALLS_EVERY_WRAPPER = (
            "listAgents(); installAgent(); createTrigger(); getAgentDetail(); installCli();\n"
        )

        text, code = measure_files(
            {"lib/ipc/agents.ts": WRAPPED_EVERY_COMMAND, "lib/consumer.ts": CALLS_EVERY_WRAPPER}
        )
        results.append(
            _case(
                "positive control: a wrapped, called, well-named layer is green",
                code == 0 and "invoke sites in .svelte       : 0" in text,
            )
        )

        text, code = measure_files(
            {
                "lib/ipc/agents.ts": WRAPPED_EVERY_COMMAND,
                "lib/consumer.ts": CALLS_EVERY_WRAPPER,
                "components/Subject.svelte": 'await invoke("list_agents");\n',
            }
        )
        results.append(
            _case(
                "an invoke in a .svelte file is a red, and the guard names the site",
                code == 1 and "components/Subject.svelte:1" in text,
            )
        )

        text, code = measure_files(
            {
                "lib/ipc/agents.ts": WRAPPED_EVERY_COMMAND,
                "lib/consumer.ts": "listAgents(); installAgent(); createTrigger(); installCli();\n",
            }
        )
        results.append(
            _case(
                "a wrapper carrying an invoke that nothing calls is a red",
                code == 1
                and "lib/ipc/agents.ts: getAgentDetail -> invoke ['get_agent_detail']" in text,
            )
        )
        results.append(
            _case(
                "and the command it alone reached is reported dead",
                "get_agent_detail  <- getAgentDetail" in text,
            )
        )

        text, code = measure_files(
            {
                "lib/ipc/agents.ts": WRAPPED_EVERY_COMMAND.replace(
                    "function getAgentDetail", "function fetchDetail"
                ),
                "lib/consumer.ts": CALLS_EVERY_WRAPPER.replace(
                    "getAgentDetail()", "fetchDetail()"
                ),
            }
        )
        results.append(
            _case(
                "a wrapper not named camelCase of its command is a red",
                code == 1
                and "fetchDetail wraps get_agent_detail, expected getAgentDetail" in text,
            )
        )

        text, code = measure_files(
            {
                "lib/ipc/agents.ts": WRAPPED_EVERY_COMMAND
                + "export function normalizeDetail(raw) { return raw; }\n",
                "lib/ipc/agents.test.ts": "normalizeDetail({});\n",
                "lib/consumer.ts": CALLS_EVERY_WRAPPER,
            }
        )
        results.append(
            _case(
                "an invoke-less helper whose only caller is a test stays green",
                code == 0,
            )
        )

        text, code = measure_files(
            {
                "lib/ipc/agents.ts": WRAPPED_EVERY_COMMAND
                + "export function normalizeDetail(raw) { return raw; }\n",
                "lib/consumer.ts": CALLS_EVERY_WRAPPER,
            }
        )
        results.append(
            _case(
                "the same helper with no caller at all is a red",
                code == 1
                and "lib/ipc/agents.ts: normalizeDetail -> invoke []" in text,
            )
        )

        shutil.rmtree(ui_root, ignore_errors=True)
        ui_root.mkdir(parents=True)
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            empty_verdict = report(rust_root, ui_root)
        results.append(
            _case("a subject with no invoke call measures nothing", empty_verdict == 2)
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
    if "--selftest" in sys.argv[1:]:
        sys.exit(selftest())
    rust_root, ui_root = REPO_ROOT / RUST_SUBTREE, REPO_ROOT / UI_SUBTREE
    for subtree in (rust_root, ui_root):
        if not subtree.is_dir():
            print(f"NOTHING MEASURED: {subtree} is absent from this tree.")
            sys.exit(2)
    sys.exit(report(rust_root, ui_root))


if __name__ == "__main__":
    main()
