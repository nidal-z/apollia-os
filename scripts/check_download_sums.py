#!/usr/bin/env python3
"""Every download the tree performs is verified against a pinned sha256 sum.

Six surfaces are held to the same rule:

  1. The llama-server assets: every asset `packaging/fetch-llama-server.sh`
     can select carries a 64-hex sum in `packaging/llama-server-checksums.txt`
     under the same tag, and no pinned sum is an orphan.
  2. The python-build-standalone archives: every target triple
     `packaging/fetch-python-standalone.sh` can select maps to an archive
     summed in `packaging/python-standalone-checksums.txt`, and the fetch
     script actually verifies against that file.
  3. The bundled Python requirements: every logical line of
     `packaging/requirements-bundled.txt` (the agents' packages) and of
     `packaging/requirements-mlx-engine.txt` (the MLX engine) is pinned with
     `==` and carries at least one `--hash=sha256:`; the script that installs
     each file does so with `--require-hashes`, and the bundle build pins its
     own pip. The MLX engine set names none of its excluded packages
     (MLX_EXCLUDED) and is installed with `--no-deps`, so an excluded package
     cannot come back through the closure.
  4. The engine channel: every source of every package pinned in
     `packaging/engine-sources.json` is an https address with a 64-hex sum,
     no archive is pinned twice with two sums, every `variant` is one of the
     `VARIANTS` of `packaging/build-engine-manifest.py`, and that script,
     which publishes them under a signed manifest, reads that file and
     compares each digest to its pin.
  5. The workflow `run:` blocks: a step that downloads with wget, curl or
     Invoke-WebRequest either verifies a sha256 in the same step or sits on
     the named exemption list below, which only ever shrinks.
  6. The Tesseract and Leptonica sources: the build script of `tesseract-rs`
     downloads them itself, unchecked, unless they are already staged.
     `packaging/tesseract-sources-checksums.txt` opens with the `tesseract-rs`
     version `Cargo.toml` pins, then gives each archive an https address, a
     64-hex sum and the 40-hex commit of its tag;
     `packaging/fetch-tesseract-sources.sh` reads that table and refuses a
     digest that differs from it; and in every workflow, a step that builds
     with the `ocr` feature (`--features` naming it, or a variable assigned
     a value that names it) comes after a step of the same job that runs
     that script.

Options:
  --github    also compare each pinned sum with the digest GitHub reports for
              the release asset (one `gh api` call per release, no download),
              and each pinned Tesseract source commit with the commit its
              upstream tag points to. CI only: it needs the network and a
              token.
  --selftest  run the checks against altered copies of their subjects and
              fail unless every alteration is caught.

Exit 0 when every surface is coherent, 1 when a defect was found, 2 when a
subject is unreadable (nothing measured, which is not a pass).
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(
    subprocess.check_output(
        ["git", "rev-parse", "--show-toplevel"], text=True
    ).strip()
)

LLAMA_FETCH = "packaging/fetch-llama-server.sh"
LLAMA_SUMS = "packaging/llama-server-checksums.txt"
PBS_FETCH = "packaging/fetch-python-standalone.sh"
PBS_SUMS = "packaging/python-standalone-checksums.txt"
REQUIREMENTS = "packaging/requirements-bundled.txt"
BUNDLE_BUILD = "packaging/build-python-bundle.sh"
MLX_REQUIREMENTS = "packaging/requirements-mlx-engine.txt"
MLX_BUILD = "packaging/build-mlx-engine.sh"
# Packages the MLX engine declares but the bundle never ships. mlx-embeddings
# is GPL-3.0 and only serves the embeddings endpoint; the engine set is
# installed with --no-deps so that leaving it out holds.
MLX_EXCLUDED = ("mlx-embeddings",)
ENGINE_SOURCES = "packaging/engine-sources.json"
ENGINE_BUILD = "packaging/build-engine-manifest.py"
TESS_SUMS = "packaging/tesseract-sources-checksums.txt"
TESS_FETCH = "packaging/fetch-tesseract-sources.sh"
CARGO_TOML = "Cargo.toml"
WORKFLOW_DIR = ".github/workflows"
SCRIPT_DIRS = ("packaging", "crates/apollia-desktop/scripts")

# Workflow steps that download without a pinned sum, named one by one so the
# list can only shrink: removing an entry is the fix, adding one is a defect
# this guard exists to refuse. The three remaining fetches feed apt's keyring
# and predate this guard; the toolchain installers that used to sit here are
# now verified against pinned sums inside release.yml itself, and the ROCm
# entry left with the step it named when the release dropped that preset.
EXEMPT_WORKFLOW_STEPS = {
    ("release.yml", "Install Vulkan SDK (Linux)"),
    ("weekly.yml", "Install ROCm"),
    ("weekly.yml", "Install Vulkan SDK"),
}

SHA256_HEX = re.compile(r"^[0-9a-f]{64}$")
DOWNLOAD_CMD = re.compile(r"(?:^|[|&;(\s])(?:wget|curl|Invoke-WebRequest)\b")
REMOTE_URL = re.compile(r"https?://(?!127\.0\.0\.1|localhost)\S+")
PUBLISHED_VARIANTS = re.compile(r"^VARIANTS = \(([^)]*)\)", re.MULTILINE)
VERIFY_TOKEN = re.compile(
    r"sha256sum|shasum\s+-a\s*256|sha256_of|Get-FileHash|--require-hashes"
)
COMMIT_HEX = re.compile(r"^[0-9a-f]{40}$")
TESS_CRATE_PIN = re.compile(
    r'^tesseract-rs\s*=\s*(?:"=?([^"]+)"|\{[^}]*\bversion\s*=\s*"=?([^"]+)")',
    re.MULTILINE,
)
TESS_TAG_URL = re.compile(
    r"^https://github\.com/([^/]+)/([^/]+)/archive/refs/tags/([^/]+)\.zip$"
)
# The `ocr` feature as cargo reads it: alone or behind a crate path
# (`apollia-sources/ocr`), never a longer name that merely contains it.
OCR_FEATURE = re.compile(r"(?<![\w-])ocr(?![\w-])")
FEATURES_FLAG = re.compile(
    r"(?:--features(?:=|\s+)|(?<!\S)-F\s*)"
    r"(\"[^\"]*\"|'[^']*'|(?:\$\{\{[^}]*\}\}|[^\s\"'])+)"
)
EXPR_REF = re.compile(r"\$\{\{\s*([\w.-]+)\s*\}\}")
SHELL_REF = re.compile(r"\$\{?([A-Za-z_]\w*)\}?")
TESS_FETCH_CALL = re.compile(r"(?:^|[\s;&|(/])packaging/fetch-tesseract-sources\.sh\b")


def _read(path: Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as err:
        print(f"unreadable subject {path}: {err}", file=sys.stderr)
        return None


def parse_sums(text: str, subject: str) -> tuple[str | None, dict[str, str], int]:
    """Tag, {asset: sum} and malformed-line count of a checksums file."""
    tag_m = re.search(r"^# Tag: (\S+)", text, re.M)
    pinned: dict[str, str] = {}
    malformed = 0
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2:
            print(f"  ECART  {subject}: malformed line: {line!r}")
            malformed += 1
            continue
        pinned[parts[1]] = parts[0]
    return (tag_m.group(1) if tag_m else None, pinned, malformed)


def check_pinned_set(
    expected: dict[str, str],
    pinned: dict[str, str],
    tag: str,
    file_tag: str | None,
    subject: str,
    tag_exempt: frozenset[str] = frozenset(),
) -> int:
    """Compare selectable archives against pinned sums, both directions."""
    problems = 0
    print(f"  script tag={tag} file tag={file_tag or 'absent'}")
    if file_tag != tag:
        print(f"  ECART  {subject}: sums file tag differs from the script tag")
        problems += 1
    for label, asset in sorted(expected.items()):
        s = pinned.get(asset)
        if s is None:
            print(f"  GAP    {label:34s} {asset}  no sum")
            problems += 1
        elif not SHA256_HEX.fullmatch(s):
            print(f"  GAP    {label:34s} {asset}  malformed sum")
            problems += 1
        else:
            print(f"  ok     {label:34s} {asset}")
    for asset in pinned:
        if asset not in expected.values():
            print(f"  GAP    {subject}: orphan sum: {asset}")
            problems += 1
        elif tag not in asset and asset not in tag_exempt:
            print(f"  GAP    {subject}: sum outside tag {tag}: {asset}")
            problems += 1
    return problems


def check_llama(script_path: Path, sums_path: Path) -> int:
    print("llama-server: pinned release assets")
    script = _read(script_path)
    sums = _read(sums_path)
    if script is None or sums is None:
        sys.exit(2)
    tag_m = re.search(r'LLAMA_CPP_TAG="\$\{LLAMA_CPP_TAG:-([^}]+)\}"', script)
    if not tag_m:
        print("tag unreadable in fetch-llama-server.sh", file=sys.stderr)
        sys.exit(2)
    tag = tag_m.group(1)
    selectable = {
        m.group(1).strip(): m.group(2).replace("${LLAMA_CPP_TAG}", tag)
        for m in re.finditer(
            r'^\s*([A-Za-z0-9*:| ]+?)\)\s+asset="([^"]+)"', script, re.M
        )
    }
    # Companion archives (the Windows CUDA runtime): selected next to a main
    # asset, verified against the same sums file, but named by upstream
    # without the llama.cpp tag, so the same-tag rule does not apply to them.
    companions = {
        f"companion {m.group(1)}": m.group(1)
        for m in re.finditer(r'^\s*extra_asset="([^"$]+)"', script, re.M)
    }
    selectable.update(companions)
    if not selectable:
        print("no selectable asset in fetch-llama-server.sh", file=sys.stderr)
        sys.exit(2)
    file_tag, pinned, problems = parse_sums(sums, sums_path.name)
    return problems + check_pinned_set(
        selectable,
        pinned,
        tag,
        file_tag,
        sums_path.name,
        tag_exempt=frozenset(companions.values()),
    )


def pbs_expected(script: str) -> tuple[str, dict[str, str]] | None:
    """Tag and {triple: archive} the python-standalone fetch can select."""
    tag_m = re.search(r'^PBS_TAG="([^"]+)"', script, re.M)
    ver_m = re.search(r'^CPYTHON_VERSION="([^"]+)"', script, re.M)
    triples = re.findall(r'PBS_TRIPLE="([^"]+)"', script)
    if not tag_m or not ver_m or not triples:
        return None
    tag, ver = tag_m.group(1), ver_m.group(1)
    return tag, {
        t: f"cpython-{ver}+{tag}-{t}-install_only.tar.gz" for t in triples
    }


def check_python_standalone(script_path: Path, sums_path: Path) -> int:
    print("python-build-standalone: pinned interpreter archives")
    script = _read(script_path)
    if script is None:
        sys.exit(2)
    parsed = pbs_expected(script)
    if parsed is None:
        print("tag, version or triples unreadable in fetch script", file=sys.stderr)
        sys.exit(2)
    tag, expected = parsed
    if not sums_path.is_file():
        print(f"  GAP    {sums_path.name}: absent, {len(expected)} archives with no sum")
        return len(expected) + 1
    sums = _read(sums_path)
    if sums is None:
        sys.exit(2)
    file_tag, pinned, problems = parse_sums(sums, sums_path.name)
    problems += check_pinned_set(expected, pinned, tag, file_tag, sums_path.name)
    # A sums file nothing reads is decorative: the fetch script must name it
    # and compare with a sha256 tool.
    if sums_path.name not in script or not VERIFY_TOKEN.search(script):
        print(f"  GAP    {script_path.name}: does not verify against {sums_path.name}")
        problems += 1
    return problems


def requirement_lines(text: str) -> list[str]:
    """Logical (backslash-joined) non-comment lines of a requirements file."""
    logical: list[str] = []
    buffer = ""
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        buffer += " " + line.rstrip("\\") if buffer else line.rstrip("\\")
        if not line.endswith("\\"):
            logical.append(buffer.strip())
            buffer = ""
    if buffer:
        logical.append(buffer.strip())
    return logical


def _normalize(name: str) -> str:
    """PEP 503 form of a distribution name: lower case, runs of -_. as -."""
    return re.sub(r"[-_.]+", "-", name).lower()


def check_requirements(
    req_path: Path,
    build_path: Path,
    pip_pinned_by: Path | None = None,
    excluded: tuple[str, ...] = (),
) -> int:
    """Pinned and summed lines, a hash-requiring install, and a pinned pip.

    `pip_pinned_by` names the script that pins pip when it is not the one
    that installs this file: the MLX engine script runs with the pip the
    bundle build already pinned. `excluded` names packages that must never
    ship: no line may pin one, and the install must not resolve the closure.
    """
    print(f"bundled requirements: {req_path.name} pinned and hashed")
    req = _read(req_path)
    build = _read(build_path)
    if req is None or build is None:
        sys.exit(2)
    problems = 0
    lines = requirement_lines(req)
    if not lines:
        print(f"  GAP    {req_path.name}: no requirement, nothing to verify")
        problems += 1
    for line in lines:
        name = line.split("==")[0].split()[0]
        if "==" not in line:
            print(f"  GAP    {name}: version not pinned: {line[:60]}")
            problems += 1
        hashes = re.findall(r"--hash=sha256:([0-9a-f]+)", line)
        if not hashes:
            print(f"  GAP    {name}: no --hash=sha256: sum")
            problems += 1
        elif any(len(h) != 64 for h in hashes):
            print(f"  GAP    {name}: malformed sum")
            problems += 1
        else:
            print(f"  ok     {name}: {len(hashes)} sum(s)")
        if _normalize(name) in {_normalize(e) for e in excluded}:
            print(f"  GAP    {name}: excluded package pinned")
            problems += 1
    if excluded and "--no-deps" not in build:
        print(
            f"  GAP    {build_path.name}: pip without --no-deps would pull "
            f"the excluded {', '.join(excluded)} back"
        )
        problems += 1
    if "--require-hashes" not in build:
        print(f"  GAP    {build_path.name}: pip install without --require-hashes")
        problems += 1
    if req_path.name not in build:
        print(f"  GAP    {build_path.name}: does not install {req_path.name}")
        problems += 1
    pip_text = build if pip_pinned_by is None else _read(pip_pinned_by)
    if pip_text is None:
        sys.exit(2)
    pip_name = (pip_pinned_by or build_path).name
    if not re.search(r"pip==\d", pip_text):
        print(f"  GAP    {pip_name}: pip not pinned")
        problems += 1
    return problems


def workflow_steps(text: str) -> list[tuple[str, str]]:
    """(step name, run block) pairs of one workflow, without a yaml library.

    Good enough for the workflows of this tree: steps open with `- name:` or
    `- uses:`, and `run:` values are inline or literal blocks.
    """
    steps: list[tuple[str, str]] = []
    name = ""
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i]
        name_m = re.match(r"^\s*-\s+name:\s*(.+?)\s*$", line)
        if name_m:
            name = name_m.group(1).strip("'\"")
            i += 1
            continue
        run_m = re.match(r"^(\s*)(?:-\s+)?run:\s*(.*)$", line)
        if run_m is None:
            i += 1
            continue
        indent, value = len(run_m.group(1)), run_m.group(2)
        i += 1
        if value and not value.startswith(("|", ">")):
            steps.append((name, value))
            continue
        block: list[str] = []
        while i < len(lines):
            nxt = lines[i]
            if nxt.strip() and len(nxt) - len(nxt.lstrip()) <= indent:
                break
            block.append(nxt)
            i += 1
        steps.append((name, "\n".join(block)))
    return steps


def download_lines(run_block: str) -> list[str]:
    """Logical lines of a run block that download from a remote URL.

    The URL can sit on the line itself, or in a variable assigned elsewhere in
    the same block (`$url = "https://..."` then `Invoke-WebRequest -Uri $url`,
    `curl "$URL"`). A line whose only URL is local (127.0.0.1, localhost) is
    not a download of interest.
    """
    logical = _logical_lines(run_block)
    block_has_remote = REMOTE_URL.search(run_block) is not None
    return [
        line
        for line in logical
        if DOWNLOAD_CMD.search(line)
        and (
            REMOTE_URL.search(line)
            or ("$" in line and "://" not in line and block_has_remote)
        )
    ]


def check_workflows(
    workflow_dir: Path, exempt: set[tuple[str, str]]
) -> int:
    print("workflows: every downloading run step verifies or is exempted")
    files = sorted(workflow_dir.glob("*.yml")) + sorted(workflow_dir.glob("*.yaml"))
    if not files:
        print(f"no workflow under {workflow_dir}", file=sys.stderr)
        sys.exit(2)
    problems = 0
    used: set[tuple[str, str]] = set()
    for path in files:
        text = _read(path)
        if text is None:
            sys.exit(2)
        for name, run in workflow_steps(text):
            downloads = download_lines(run)
            if not downloads:
                continue
            if VERIFY_TOKEN.search(run):
                print(f"  ok     {path.name}: {name or '(unnamed)'}: verified")
                continue
            if (path.name, name) in exempt:
                used.add((path.name, name))
                print(f"  exempt {path.name}: {name}")
                continue
            print(
                f"  GAP    {path.name}: step {name or '(unnamed)'!r} downloads "
                f"without verification: {downloads[0][:90]}"
            )
            problems += 1
    for entry in sorted(exempt - used):
        print(f"  GAP    stale exemption (no download left): {entry}")
        problems += 1
    print(f"  named exemptions: {len(exempt)}, used: {len(used)}")
    return problems


def check_engine_sources(sources_path: Path, script_path: Path) -> int:
    """The archives the engine channel is published from are all pinned."""
    print("engine channel: pinned upstream archives")
    text = _read(sources_path)
    script = _read(script_path)
    if text is None or script is None:
        sys.exit(2)
    try:
        packages = json.loads(text).get("packages")
    except json.JSONDecodeError as err:
        print(f"unreadable {sources_path.name}: {err}", file=sys.stderr)
        sys.exit(2)
    if not isinstance(packages, list) or not packages:
        print(f"no package in {sources_path.name}", file=sys.stderr)
        sys.exit(2)
    problems = 0
    declared = PUBLISHED_VARIANTS.search(script)
    variants = set(re.findall(r'"([^"]+)"', declared.group(1))) if declared else set()
    if not variants:
        print(f"  GAP    {script_path.name} declares no VARIANTS tuple")
        problems += 1
    sums: dict[str, str] = {}
    for package in packages:
        label = "{} {} {} {}".format(
            package.get("engine"), package.get("tag"),
            package.get("variant"), package.get("platform"),
        )
        if variants and package.get("variant") not in variants:
            print(
                f"  GAP    {label}: variant {package.get('variant')!r} is not "
                f"one {script_path.name} publishes"
            )
            problems += 1
        sources = package.get("sources") or []
        if not sources:
            print(f"  GAP    {label}: no source")
            problems += 1
        for source in sources:
            url = str(source.get("url", ""))
            pinned = str(source.get("sha256", ""))
            if not url.startswith("https://"):
                print(f"  GAP    {label}: {url or '(no address)'} is not https")
                problems += 1
            elif not SHA256_HEX.fullmatch(pinned):
                print(f"  GAP    {label}: {url}  no 64-hex sum")
                problems += 1
            elif sums.setdefault(url, pinned) != pinned:
                print(f"  GAP    {label}: {url} is pinned with two different sums")
                problems += 1
            else:
                print(f"  ok     {label:44s} {url.rsplit('/', 1)[-1]}")
    body = "\n".join(
        line for line in script.splitlines() if not line.lstrip().startswith("#")
    )
    if sources_path.name not in body or "differs from the pinned" not in body:
        print(
            f"  GAP    {script_path.name} does not read {sources_path.name} "
            "and refuse a digest that differs from its pin"
        )
        problems += 1
    return problems


def check_fetch_scripts(root: Path, dirs: tuple[str, ...]) -> int:
    print("shell scripts: every downloading script names a sums verification")
    problems = 0
    scanned = 0
    for rel in dirs:
        base = root / rel
        if not base.is_dir():
            print(f"missing script dir {base}", file=sys.stderr)
            sys.exit(2)
        for path in sorted(base.rglob("*.sh")) + sorted(base.rglob("*.ps1")):
            text = _read(path)
            if text is None:
                sys.exit(2)
            scanned += 1
            body = "\n".join(
                line
                for line in text.splitlines()
                if not line.lstrip().startswith("#")
            )
            downloads = download_lines(body)
            if not downloads:
                continue
            if "-checksums.txt" in text and VERIFY_TOKEN.search(text):
                print(f"  ok     {path.relative_to(root)}")
            else:
                print(
                    f"  GAP    {path.relative_to(root)}: downloads without a "
                    f"sum table: {downloads[0].strip()[:90]}"
                )
                problems += 1
    print(f"  scripts scanned: {scanned}")
    return problems


def parse_tesseract_sums(
    text: str,
) -> tuple[str | None, list[tuple[str, str, str]], list[str]]:
    """Header version, (sum, url, commit) lines, and malformed lines."""
    version: str | None = None
    entries: list[tuple[str, str, str]] = []
    malformed: list[str] = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if version is None and not entries and parts[0] == "tesseract-rs":
            version = parts[1] if len(parts) == 2 else ""
            continue
        if len(parts) != 3:
            malformed.append(line)
            continue
        entries.append((parts[0], parts[1], parts[2]))
    return version, entries, malformed


def check_tesseract_sources(
    sums_path: Path, fetch_path: Path, cargo_path: Path
) -> int:
    """Rules (a), (b) and (c) of the sixth surface."""
    print("tesseract sources: pinned archives for the tesseract-rs build script")
    sums = _read(sums_path)
    fetch = _read(fetch_path)
    cargo = _read(cargo_path)
    if sums is None or fetch is None or cargo is None:
        sys.exit(2)
    problems = 0
    pin_m = TESS_CRATE_PIN.search(cargo)
    crate_version = (pin_m.group(1) or pin_m.group(2)) if pin_m else None
    version, entries, malformed = parse_tesseract_sums(sums)
    print(f"  Cargo.toml tesseract-rs={crate_version or 'absent'} table={version or 'absent'}")
    if crate_version is None:
        print(f"  GAP    {cargo_path.name}: no tesseract-rs pin to compare with")
        problems += 1
    elif version != crate_version:
        print(
            f"  GAP    {sums_path.name}: serves tesseract-rs {version or '(no header)'}, "
            f"{cargo_path.name} pins {crate_version}"
        )
        problems += 1
    for line in malformed:
        print(f"  GAP    {sums_path.name}: malformed line: {line[:90]!r}")
        problems += 1
    if not entries and not malformed:
        print(f"  GAP    {sums_path.name}: no archive pinned")
        problems += 1
    for s, url, commit in entries:
        label = url.rsplit("/", 1)[-1]
        if not url.startswith("https://"):
            print(f"  GAP    {url}: not https")
            problems += 1
        elif not SHA256_HEX.fullmatch(s):
            print(f"  GAP    {url}: no 64-hex sum")
            problems += 1
        elif not COMMIT_HEX.fullmatch(commit):
            print(f"  GAP    {url}: no 40-hex commit")
            problems += 1
        else:
            print(f"  ok     {label}  {commit[:12]}")
    body = "\n".join(
        line for line in fetch.splitlines() if not line.lstrip().startswith("#")
    )
    if (
        sums_path.name not in body
        or not VERIFY_TOKEN.search(body)
        or "differs from the pinned" not in body
    ):
        print(
            f"  GAP    {fetch_path.name} does not read {sums_path.name} and "
            "refuse a digest that differs from its pin"
        )
        problems += 1
    return problems


def workflow_jobs(text: str) -> tuple[str, list[tuple[str, str]]]:
    """Text before `jobs:`, and (job id, job text) pairs, without a yaml library."""
    lines = text.splitlines()
    start = next(
        (i for i, line in enumerate(lines) if re.match(r"^jobs:\s*(#.*)?$", line)),
        None,
    )
    if start is None:
        return text, []
    header = "\n".join(lines[:start])
    jobs: list[tuple[str, list[str]]] = []
    job_indent: int | None = None
    for line in lines[start + 1 :]:
        if line.strip() and not line.startswith((" ", "#")):
            break
        head = re.match(r"^( +)([A-Za-z0-9_-]+):\s*(#.*)?$", line)
        if head and (job_indent is None or len(head.group(1)) == job_indent):
            job_indent = len(head.group(1))
            jobs.append((head.group(2), []))
            continue
        if jobs:
            jobs[-1][1].append(line)
    return header, [(job, "\n".join(body)) for job, body in jobs]


def _logical_lines(run_block: str) -> list[str]:
    """Non-comment, backslash-joined lines of a run block."""
    logical: list[str] = []
    buffer = ""
    for raw in run_block.splitlines():
        line = raw.strip()
        if line.startswith("#"):
            continue
        buffer += " " + line.rstrip("\\") if buffer else line.rstrip("\\")
        if not line.endswith("\\"):
            logical.append(buffer)
            buffer = ""
    if buffer:
        logical.append(buffer)
    return logical


def _carries_ocr(value: str, scope: str, depth: int = 0) -> bool:
    """A features value names `ocr`, literally or through a variable.

    A variable is read as every value assigned to its name in the scope (the
    job and the workflow header): a matrix entry, an `env:` key, a shell
    assignment, a JSON field of an included matrix. Reading every assignment
    errs on the side of a red, which is the side this surface must err on.
    """
    if OCR_FEATURE.search(SHELL_REF.sub(" ", EXPR_REF.sub(" ", value))):
        return True
    if depth >= 4:
        return False
    names = {ref.rsplit(".", 1)[-1] for ref in EXPR_REF.findall(value)}
    names |= set(SHELL_REF.findall(EXPR_REF.sub(" ", value)))
    for name in names:
        assigned = re.compile(
            r"(?<![\w-])[\"']?" + re.escape(name) + r"[\"']?\s*[:=]\s*"
            r"(\"[^\"]*\"|'[^']*'|\$\([^)]*\)|[^\s,}]+)"
        )
        for m in assigned.finditer(scope):
            if _carries_ocr(m.group(1), scope, depth + 1):
                return True
    return False


def check_ocr_workflows(workflow_dir: Path) -> int:
    """Rule (d): an `ocr` build comes after the staging script, in its job."""
    print("workflows: every ocr build stages the verified Tesseract sources first")
    files = sorted(workflow_dir.glob("*.yml")) + sorted(workflow_dir.glob("*.yaml"))
    if not files:
        print(f"no workflow under {workflow_dir}", file=sys.stderr)
        sys.exit(2)
    problems = 0
    builds = 0
    for path in files:
        text = _read(path)
        if text is None:
            sys.exit(2)
        header, jobs = workflow_jobs(text)
        for job, body in jobs:
            scope = header + "\n" + body
            staged = False
            for name, run in workflow_steps(body):
                lines = _logical_lines(run)
                ocr_lines = [
                    line
                    for line in lines
                    if re.search(r"\bcargo\b", line)
                    and any(
                        _carries_ocr(v.strip("\"'"), scope)
                        for v in FEATURES_FLAG.findall(line)
                    )
                ]
                if ocr_lines:
                    builds += 1
                    label = f"{path.name}: job {job}: step {name or '(unnamed)'!r}"
                    if staged:
                        print(f"  ok     {label}")
                    else:
                        print(
                            f"  GAP    {label} builds with ocr before any step of "
                            f"its job runs {TESS_FETCH}: {ocr_lines[0][:80]}"
                        )
                        problems += 1
                if any(TESS_FETCH_CALL.search(line) for line in lines):
                    staged = True
    print(f"  ocr builds found: {builds}")
    return problems


def github_digests(repo: str, tag: str, pinned: dict[str, str]) -> int:
    out = subprocess.run(
        [
            "gh", "api", f"repos/{repo}/releases/tags/{tag}",
            "--jq", '.assets[] | "\\(.digest // "nodigest") \\(.name)"',
        ],
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        print(f"gh api unavailable for {repo}@{tag}: {out.stderr.strip()}")
        sys.exit(2)
    remote = dict(
        reversed(line.split(" ", 1))
        for line in out.stdout.splitlines()
        if " " in line
    )
    problems = 0
    for asset, s in sorted(pinned.items()):
        d = remote.get(asset)
        if d is None:
            print(f"  GAP    {asset}: absent from the GitHub release {tag}")
            problems += 1
        elif d != f"sha256:{s}":
            print(f"  GAP    {asset}: GitHub {d} != pinned {s}")
            problems += 1
        else:
            print(f"  ok     {asset}: pinned sum == GitHub digest")
    return problems


def check_github() -> int:
    print("GitHub digests: pinned sums match the upstream release assets")
    problems = 0
    script = _read(REPO_ROOT / LLAMA_FETCH)
    sums = _read(REPO_ROOT / LLAMA_SUMS)
    if script is None or sums is None:
        sys.exit(2)
    tag_m = re.search(r'LLAMA_CPP_TAG="\$\{LLAMA_CPP_TAG:-([^}]+)\}"', script)
    if tag_m:
        _, pinned, _ = parse_sums(sums, LLAMA_SUMS)
        problems += github_digests("ggml-org/llama.cpp", tag_m.group(1), pinned)
    pbs_script = _read(REPO_ROOT / PBS_FETCH)
    pbs_sums = _read(REPO_ROOT / PBS_SUMS)
    if pbs_script is None or pbs_sums is None:
        sys.exit(2)
    parsed = pbs_expected(pbs_script)
    if parsed:
        _, pinned, _ = parse_sums(pbs_sums, PBS_SUMS)
        problems += github_digests(
            "astral-sh/python-build-standalone", parsed[0], pinned
        )
    tess_sums = _read(REPO_ROOT / TESS_SUMS)
    if tess_sums is None:
        sys.exit(2)
    _, entries, _ = parse_tesseract_sums(tess_sums)
    problems += github_tag_commits(entries)
    return problems


def _gh_object(path: str) -> tuple[str, str]:
    """(type, sha) of the object a git ref or tag of the GitHub API points to."""
    out = subprocess.run(
        ["gh", "api", path, "--jq", '.object.type + " " + .object.sha'],
        capture_output=True,
        text=True,
    )
    parts = out.stdout.split()
    if out.returncode != 0 or len(parts) != 2:
        print(f"gh api unavailable for {path}: {out.stderr.strip()}")
        sys.exit(2)
    return parts[0], parts[1]


def github_tag_commits(entries: list[tuple[str, str, str]]) -> int:
    """Each pinned Tesseract source commit is the one its upstream tag names.

    The tag ref points to a commit, or for an annotated tag to a tag object
    whose own object is the commit. Nothing is downloaded.
    """
    problems = 0
    for _, url, commit in entries:
        m = TESS_TAG_URL.match(url)
        if m is None:
            print(f"  GAP    {url}: not a GitHub tag archive")
            problems += 1
            continue
        owner, repo, tag = m.groups()
        kind, sha = _gh_object(f"repos/{owner}/{repo}/git/ref/tags/{tag}")
        hops = 0
        while kind == "tag" and hops < 3:
            kind, sha = _gh_object(f"repos/{owner}/{repo}/git/tags/{sha}")
            hops += 1
        if kind != "commit":
            print(f"  GAP    {owner}/{repo}@{tag}: tag resolves to a {kind}, not a commit")
            problems += 1
        elif sha != commit:
            print(f"  GAP    {owner}/{repo}@{tag}: GitHub {sha} != pinned {commit}")
            problems += 1
        else:
            print(f"  ok     {owner}/{repo}@{tag}: pinned commit == upstream tag")
    return problems


def selftest() -> int:
    """Each check fires on an altered copy of its subject, and only then."""
    import tempfile

    failures = 0

    def control(label: str, fired: bool) -> None:
        nonlocal failures
        if fired:
            print(f"  ok     selftest: {label}")
        else:
            print(f"  GAP    selftest: {label}: the alteration went unseen")
            failures += 1

    with tempfile.TemporaryDirectory() as tmp_s:
        tmp = Path(tmp_s)

        real = _read(REPO_ROOT / LLAMA_SUMS) or ""
        altered = tmp / "llama-sums.txt"
        altered.write_text(
            "\n".join(real.splitlines()[:-1]), encoding="utf-8"
        )
        control(
            "a removed llama sum is a gap",
            check_llama(REPO_ROOT / LLAMA_FETCH, altered) > 0,
        )

        pbs_real = _read(REPO_ROOT / PBS_SUMS)
        if pbs_real is None:
            control("the python-standalone sum file exists", False)
        else:
            corrupt = tmp / "pbs-sums.txt"
            corrupt.write_text(
                re.sub(r"^([0-9a-f]{63})[0-9a-f]", r"\1z", pbs_real, count=1, flags=re.M),
                encoding="utf-8",
            )
            control(
                "a corrupted python-standalone sum is a gap",
                check_python_standalone(REPO_ROOT / PBS_FETCH, corrupt) > 0,
            )

        req = tmp / "requirements.txt"
        req.write_text("pypdf==6.0.0\n", encoding="utf-8")
        build_ok = tmp / "build.sh"
        build_ok.write_text(
            "pip install pip==99.0\npip install --require-hashes -r requirements.txt\n",
            encoding="utf-8",
        )
        control(
            "a requirement without --hash is a gap",
            check_requirements(req, build_ok) > 0,
        )
        req.write_text(
            "pypdf==6.0.0 --hash=sha256:" + "0" * 64 + "\n", encoding="utf-8"
        )
        control(
            "a pinned and summed requirement passes",
            check_requirements(req, build_ok) == 0,
        )
        build_bad = tmp / "build-bad.sh"
        build_bad.write_text("pip install -r requirements.txt\n", encoding="utf-8")
        control(
            "a pip install without --require-hashes is a gap",
            check_requirements(req, build_bad) > 0,
        )
        engine_build = tmp / "build-engine.sh"
        engine_build.write_text(
            "pip install --require-hashes --target t -r requirements.txt\n",
            encoding="utf-8",
        )
        control(
            "a second set whose pip is pinned by the bundle build passes",
            check_requirements(req, engine_build, pip_pinned_by=build_ok) == 0,
        )
        control(
            "a second set with no pinned pip anywhere is a gap",
            check_requirements(req, engine_build, pip_pinned_by=build_bad) > 0,
        )
        nodeps_build = tmp / "build-engine-nodeps.sh"
        nodeps_build.write_text(
            "pip install --require-hashes --no-deps --target t -r requirements.txt\n",
            encoding="utf-8",
        )
        control(
            "a second set installed with --no-deps and no excluded line passes",
            check_requirements(
                req, nodeps_build, pip_pinned_by=build_ok, excluded=("evil-pkg",)
            )
            == 0,
        )
        control(
            "a set with exclusions installed without --no-deps is a gap",
            check_requirements(
                req, engine_build, pip_pinned_by=build_ok, excluded=("evil-pkg",)
            )
            > 0,
        )
        excluded_req = tmp / "requirements-excluded.txt"
        excluded_req.write_text(
            req.read_text(encoding="utf-8")
            + "Evil_Pkg==1.0 \\\n    --hash=sha256:" + "b" * 64 + "\n",
            encoding="utf-8",
        )
        nodeps_excluded = tmp / "build-engine-excluded.sh"
        nodeps_excluded.write_text(
            "pip install --require-hashes --no-deps --target t "
            "-r requirements-excluded.txt\n",
            encoding="utf-8",
        )
        control(
            "a set that pins an excluded package is a gap",
            check_requirements(
                excluded_req,
                nodeps_excluded,
                pip_pinned_by=build_ok,
                excluded=("evil-pkg",),
            )
            > 0,
        )
        other_build = tmp / "build-other.sh"
        other_build.write_text(
            "pip install pip==99.0\npip install --require-hashes -r other.txt\n",
            encoding="utf-8",
        )
        control(
            "a requirements file no build script installs is a gap",
            check_requirements(req, other_build) > 0,
        )

        engine_script = tmp / "build-engine-manifest.py"
        engine_script.write_text(
            'SOURCES = "engine-sources.json"\n'
            'VARIANTS = ("cuda-12", "cpu")\n'
            'raise SystemExit("sha256 differs from the pinned one")\n',
            encoding="utf-8",
        )
        engine_pins = tmp / "engine-sources.json"

        def engine_pin(url: str, pinned: str, variant: str = "cuda-12") -> None:
            engine_pins.write_text(
                json.dumps(
                    {
                        "packages": [
                            {
                                "engine": "llama-server", "tag": "b1",
                                "variant": variant, "platform": "linux-x64",
                                "sources": [{"url": url, "sha256": pinned}],
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )

        engine_pin("https://example.invalid/a.zip", "a" * 64)
        control(
            "a pinned engine source passes",
            check_engine_sources(engine_pins, engine_script) == 0,
        )
        engine_pin("https://example.invalid/a.zip", "")
        control(
            "an engine source without a sum is a gap",
            check_engine_sources(engine_pins, engine_script) > 0,
        )
        engine_pin("http://example.invalid/a.zip", "a" * 64)
        control(
            "an engine source over plain http is a gap",
            check_engine_sources(engine_pins, engine_script) > 0,
        )
        engine_pin("https://example.invalid/a.zip", "a" * 64, "cuda")
        control(
            "a pin whose variant the publication script does not publish is a gap",
            check_engine_sources(engine_pins, engine_script) > 0,
        )
        engine_pin("https://example.invalid/a.zip", "a" * 64)
        engine_script.write_text(
            '# engine-sources.json\nVARIANTS = ("cuda-12", "cpu")\n', encoding="utf-8"
        )
        control(
            "a publication script that compares no digest is a gap",
            check_engine_sources(engine_pins, engine_script) > 0,
        )

        wf = tmp / "workflows"
        wf.mkdir()
        (wf / "fixture.yml").write_text(
            "jobs:\n  a:\n    steps:\n"
            "      - name: Fetch tool\n"
            "        run: |\n"
            "          wget https://example.invalid/tool.deb -O /tmp/tool.deb\n",
            encoding="utf-8",
        )
        control(
            "an unexempted workflow download is a gap",
            check_workflows(wf, set()) > 0,
        )
        control(
            "the same download, exempted by name, passes",
            check_workflows(wf, {("fixture.yml", "Fetch tool")}) == 0,
        )
        (wf / "fixture.yml").write_text(
            "jobs:\n  a:\n    steps:\n"
            "      - name: Fetch tool\n"
            "        run: |\n"
            "          wget https://example.invalid/tool.deb -O /tmp/tool.deb\n"
            "          echo expected  /tmp/tool.deb | sha256sum -c -\n",
            encoding="utf-8",
        )
        control(
            "a stale exemption is a gap",
            check_workflows(wf, {("fixture.yml", "Fetch tool")}) > 0,
        )

        tess_real = _read(REPO_ROOT / TESS_SUMS) or ""
        tess_fetch = REPO_ROOT / TESS_FETCH
        cargo = REPO_ROOT / CARGO_TOML
        control(
            "the pinned Tesseract sources of the tree pass",
            check_tesseract_sources(REPO_ROOT / TESS_SUMS, tess_fetch, cargo) == 0,
        )
        tess_alt = tmp / "tesseract-sources-checksums.txt"

        def tess_variant(label: str, text: str) -> None:
            tess_alt.write_text(text, encoding="utf-8")
            control(label, check_tesseract_sources(tess_alt, tess_fetch, cargo) > 0)

        tess_variant(
            "a table serving another tesseract-rs than Cargo.toml pins is a gap",
            re.sub(r"^tesseract-rs \S+", "tesseract-rs 9.9.9", tess_real, flags=re.M),
        )
        tess_variant(
            "a Tesseract source sum of 63 hex is a gap",
            re.sub(r"^([0-9a-f]{63})[0-9a-f]", r"\1", tess_real, count=1, flags=re.M),
        )
        tess_variant(
            "a Tesseract source line without its commit is a gap",
            re.sub(r"^(\S+\s+\S+)\s+[0-9a-f]{40}$", r"\1", tess_real, count=1, flags=re.M),
        )
        tess_variant(
            "a Tesseract source over plain http is a gap",
            tess_real.replace("https://github.com/", "http://github.com/", 1),
        )
        cargo_alt = tmp / "Cargo.toml"
        cargo_alt.write_text(
            (_read(cargo) or "").replace('tesseract-rs = "=0.4.0"', 'tesseract-rs = "=0.5.0"'),
            encoding="utf-8",
        )
        control(
            "a Cargo.toml bump of tesseract-rs without a new table is a gap",
            check_tesseract_sources(REPO_ROOT / TESS_SUMS, tess_fetch, cargo_alt) > 0,
        )
        fetch_alt = tmp / "fetch-tesseract-sources.sh"
        fetch_alt.write_text(
            "\n".join(
                line
                for line in (_read(tess_fetch) or "").splitlines()
                if "CHECKSUMS=" not in line
            ).replace("tesseract-sources-checksums.txt", "other.txt"),
            encoding="utf-8",
        )
        control(
            "a staging script that does not read the table is a gap",
            check_tesseract_sources(REPO_ROOT / TESS_SUMS, fetch_alt, cargo) > 0,
        )

        owf = tmp / "ocr-workflows"
        owf.mkdir()
        stage = (
            "      - name: Stage the verified Tesseract and Leptonica sources\n"
            "        run: bash packaging/fetch-tesseract-sources.sh\n"
        )
        build = (
            "      - name: cargo test (apollia-sources, ocr)\n"
            "        run: cargo test -p apollia-sources --features ocr\n"
        )

        def ocr_variant(text: str) -> int:
            (owf / "fixture.yml").write_text(text, encoding="utf-8")
            return check_ocr_workflows(owf)

        control(
            "an ocr build after the staging step of its job passes",
            ocr_variant("jobs:\n  ocr:\n    steps:\n" + stage + build) == 0,
        )
        control(
            "an ocr build with no staging step in its job is a gap",
            ocr_variant("jobs:\n  ocr:\n    steps:\n" + build) > 0,
        )
        control(
            "an ocr build staged only by another job is a gap",
            ocr_variant(
                "jobs:\n  stage:\n    steps:\n" + stage
                + "  ocr:\n    needs: stage\n    steps:\n" + build
            )
            > 0,
        )
        control(
            "an ocr build that runs before its staging step is a gap",
            ocr_variant("jobs:\n  ocr:\n    steps:\n" + build + stage) > 0,
        )
        control(
            "a staging call left in a comment stages nothing",
            ocr_variant(
                "jobs:\n  ocr:\n    steps:\n"
                "      - name: Stage\n        run: |\n"
                "          # bash packaging/fetch-tesseract-sources.sh\n"
                "          true\n" + build
            )
            > 0,
        )
        control(
            "a features variable of the matrix carrying ocr is a gap",
            ocr_variant(
                "jobs:\n  m:\n    strategy:\n      matrix:\n        include:\n"
                "          - feature: local-cpu\n"
                "          - feature: apollia-sources/ocr\n"
                "    steps:\n"
                "      - name: cargo check\n"
                "        run: cargo check -p x --features ${{ matrix.feature }}\n"
            )
            > 0,
        )
        control(
            "a shell variable carrying ocr, on a continued line, is a gap",
            ocr_variant(
                "env:\n  FEATS: \"local-cpu,ocr\"\n"
                "jobs:\n  b:\n    steps:\n"
                "      - name: Build\n        run: |\n"
                "          cargo build --release \\\n"
                "            --features \"$FEATS\"\n"
            )
            > 0,
        )
        control(
            "a features value that only contains the letters ocr passes",
            ocr_variant(
                "jobs:\n  b:\n    steps:\n"
                "      - name: Build\n"
                "        run: cargo build --features local-cpu,ocr-extra\n"
            )
            == 0,
        )

    print(f"\nselftest: {failures} control(s) failed")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--selftest", action="store_true", help="replay the fixture controls instead of measuring the tree"
    )
    parser.add_argument(
        "--github",
        action="store_true",
        help="also compare the pinned sums against the upstream digests (network, GH_TOKEN)",
    )
    args = parser.parse_args()
    if args.selftest:
        return selftest()
    problems = 0
    problems += check_llama(REPO_ROOT / LLAMA_FETCH, REPO_ROOT / LLAMA_SUMS)
    problems += check_python_standalone(
        REPO_ROOT / PBS_FETCH, REPO_ROOT / PBS_SUMS
    )
    problems += check_requirements(
        REPO_ROOT / REQUIREMENTS, REPO_ROOT / BUNDLE_BUILD
    )
    problems += check_requirements(
        REPO_ROOT / MLX_REQUIREMENTS,
        REPO_ROOT / MLX_BUILD,
        pip_pinned_by=REPO_ROOT / BUNDLE_BUILD,
        excluded=MLX_EXCLUDED,
    )
    problems += check_engine_sources(
        REPO_ROOT / ENGINE_SOURCES, REPO_ROOT / ENGINE_BUILD
    )
    problems += check_workflows(REPO_ROOT / WORKFLOW_DIR, EXEMPT_WORKFLOW_STEPS)
    problems += check_fetch_scripts(REPO_ROOT, SCRIPT_DIRS)
    problems += check_tesseract_sources(
        REPO_ROOT / TESS_SUMS, REPO_ROOT / TESS_FETCH, REPO_ROOT / CARGO_TOML
    )
    problems += check_ocr_workflows(REPO_ROOT / WORKFLOW_DIR)
    if args.github:
        problems += check_github()
    print(f"\ngaps={problems}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
