#!/usr/bin/env python3
"""Every download the tree performs is verified against a pinned sha256 sum.

Four surfaces are held to the same rule:

  1. The llama-server assets: every asset `packaging/fetch-llama-server.sh`
     can select carries a 64-hex sum in `packaging/llama-server-checksums.txt`
     under the same tag, and no pinned sum is an orphan.
  2. The python-build-standalone archives: every target triple
     `packaging/fetch-python-standalone.sh` can select maps to an archive
     summed in `packaging/python-standalone-checksums.txt`, and the fetch
     script actually verifies against that file.
  3. The bundled Python requirements: every logical line of
     `packaging/requirements-bundled.txt` is pinned with `==` and carries at
     least one `--hash=sha256:`, and `packaging/build-python-bundle.sh`
     installs it with `--require-hashes` and pins its own pip.
  4. The workflow `run:` blocks: a step that downloads with wget, curl or
     Invoke-WebRequest either verifies a sha256 in the same step or sits on
     the named exemption list below, which only ever shrinks.

Options:
  --github    also compare each pinned sum with the digest GitHub reports for
              the release asset (one `gh api` call per release, no download).
              CI only: it needs the network and a token.
  --selftest  run the checks against altered copies of their subjects and
              fail unless every alteration is caught.

Exit 0 when every surface is coherent, 1 when a defect was found, 2 when a
subject is unreadable (nothing measured, which is not a pass).
"""

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
WORKFLOW_DIR = ".github/workflows"
SCRIPT_DIRS = ("packaging", "crates/apollia-desktop/scripts")

# Workflow steps that download without a pinned sum, named one by one so the
# list can only shrink: removing an entry is the fix, adding one is a defect
# this guard exists to refuse. The three toolchain installers execute what
# they download; the four key fetches feed apt's keyring. All seven predate
# this guard.
EXEMPT_WORKFLOW_STEPS = {
    ("release.yml", "Install CUDA aarch64 (Jetson SBSA)"),
    ("release.yml", "Install ROCm (Linux)"),
    ("release.yml", "Install HIP SDK (Windows)"),
    ("release.yml", "Install Vulkan SDK (Linux)"),
    ("release.yml", "Install Vulkan SDK (Windows)"),
    ("nightly.yml", "Install ROCm"),
    ("nightly.yml", "Install Vulkan SDK"),
}

SHA256_HEX = re.compile(r"^[0-9a-f]{64}$")
DOWNLOAD_CMD = re.compile(r"(?:^|[|&;(\s])(?:wget|curl|Invoke-WebRequest)\b")
REMOTE_URL = re.compile(r"https?://(?!127\.0\.0\.1|localhost)\S+")
VERIFY_TOKEN = re.compile(
    r"sha256sum|shasum\s+-a\s*256|sha256_of|Get-FileHash|--require-hashes"
)


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
) -> int:
    """Compare selectable archives against pinned sums, both directions."""
    problems = 0
    print(f"  tag script={tag} tag fichier={file_tag or 'absent'}")
    if file_tag != tag:
        print(f"  ECART  {subject}: sums file tag differs from the script tag")
        problems += 1
    for label, asset in sorted(expected.items()):
        s = pinned.get(asset)
        if s is None:
            print(f"  ECART  {label:34s} {asset}  sans somme")
            problems += 1
        elif not SHA256_HEX.fullmatch(s):
            print(f"  ECART  {label:34s} {asset}  somme mal formee")
            problems += 1
        else:
            print(f"  ok     {label:34s} {asset}")
    for asset in pinned:
        if asset not in expected.values():
            print(f"  ECART  {subject}: somme orpheline: {asset}")
            problems += 1
        elif tag not in asset:
            print(f"  ECART  {subject}: somme hors tag {tag}: {asset}")
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
            r'^\s*([A-Za-z*:| ]+?)\)\s+asset="([^"]+)"', script, re.M
        )
    }
    if not selectable:
        print("no selectable asset in fetch-llama-server.sh", file=sys.stderr)
        sys.exit(2)
    file_tag, pinned, problems = parse_sums(sums, sums_path.name)
    return problems + check_pinned_set(
        selectable, pinned, tag, file_tag, sums_path.name
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
        print(f"  ECART  {sums_path.name}: absent, {len(expected)} archives sans somme")
        return len(expected) + 1
    sums = _read(sums_path)
    if sums is None:
        sys.exit(2)
    file_tag, pinned, problems = parse_sums(sums, sums_path.name)
    problems += check_pinned_set(expected, pinned, tag, file_tag, sums_path.name)
    # A sums file nothing reads is decorative: the fetch script must name it
    # and compare with a sha256 tool.
    if sums_path.name not in script or not VERIFY_TOKEN.search(script):
        print(f"  ECART  {script_path.name}: ne verifie pas contre {sums_path.name}")
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


def check_requirements(req_path: Path, build_path: Path) -> int:
    print("bundled requirements: pinned and hashed")
    req = _read(req_path)
    build = _read(build_path)
    if req is None or build is None:
        sys.exit(2)
    problems = 0
    lines = requirement_lines(req)
    if not lines:
        print(f"  ECART  {req_path.name}: aucune exigence, rien a verifier")
        problems += 1
    for line in lines:
        name = line.split("==")[0].split()[0]
        if "==" not in line:
            print(f"  ECART  {name}: version non epinglee: {line[:60]}")
            problems += 1
        hashes = re.findall(r"--hash=sha256:([0-9a-f]+)", line)
        if not hashes:
            print(f"  ECART  {name}: aucune somme --hash=sha256:")
            problems += 1
        elif any(len(h) != 64 for h in hashes):
            print(f"  ECART  {name}: somme mal formee")
            problems += 1
        else:
            print(f"  ok     {name}: {len(hashes)} somme(s)")
    if "--require-hashes" not in build:
        print(f"  ECART  {build_path.name}: pip install sans --require-hashes")
        problems += 1
    if not re.search(r"pip==\d", build):
        print(f"  ECART  {build_path.name}: pip non epingle")
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
                print(f"  ok     {path.name}: {name or '(sans nom)'}: verifie")
                continue
            if (path.name, name) in exempt:
                used.add((path.name, name))
                print(f"  exempt {path.name}: {name}")
                continue
            print(
                f"  ECART  {path.name}: step {name or '(sans nom)'!r} telecharge "
                f"sans verification: {downloads[0][:90]}"
            )
            problems += 1
    for entry in sorted(exempt - used):
        print(f"  ECART  exemption perimee (plus aucun telechargement): {entry}")
        problems += 1
    print(f"  exemptions nommees: {len(exempt)}, utilisees: {len(used)}")
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
                    f"  ECART  {path.relative_to(root)}: telecharge sans table "
                    f"de sommes: {downloads[0].strip()[:90]}"
                )
                problems += 1
    print(f"  scripts parcourus: {scanned}")
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
        print(f"gh api indisponible pour {repo}@{tag}: {out.stderr.strip()}")
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
            print(f"  ECART  {asset}: absent de la release GitHub {tag}")
            problems += 1
        elif d != f"sha256:{s}":
            print(f"  ECART  {asset}: GitHub {d} != epingle {s}")
            problems += 1
        else:
            print(f"  ok     {asset}: somme epinglee == digest GitHub")
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
            print(f"  ECART  selftest: {label}: l'alteration n'a pas ete vue")
            failures += 1

    with tempfile.TemporaryDirectory() as tmp_s:
        tmp = Path(tmp_s)

        real = _read(REPO_ROOT / LLAMA_SUMS) or ""
        altered = tmp / "llama-sums.txt"
        altered.write_text(
            "\n".join(real.splitlines()[:-1]), encoding="utf-8"
        )
        control(
            "une somme llama retiree est un ecart",
            check_llama(REPO_ROOT / LLAMA_FETCH, altered) > 0,
        )

        pbs_real = _read(REPO_ROOT / PBS_SUMS)
        if pbs_real is None:
            control("le fichier de sommes python-standalone existe", False)
        else:
            corrupt = tmp / "pbs-sums.txt"
            corrupt.write_text(
                re.sub(r"^([0-9a-f]{63})[0-9a-f]", r"\1z", pbs_real, count=1, flags=re.M),
                encoding="utf-8",
            )
            control(
                "une somme python-standalone corrompue est un ecart",
                check_python_standalone(REPO_ROOT / PBS_FETCH, corrupt) > 0,
            )

        req = tmp / "requirements.txt"
        req.write_text("pypdf==6.0.0\n", encoding="utf-8")
        build_ok = tmp / "build.sh"
        build_ok.write_text(
            "pip install pip==99.0\npip install --require-hashes -r r.txt\n",
            encoding="utf-8",
        )
        control(
            "une exigence sans --hash est un ecart",
            check_requirements(req, build_ok) > 0,
        )
        req.write_text(
            "pypdf==6.0.0 --hash=sha256:" + "0" * 64 + "\n", encoding="utf-8"
        )
        control(
            "une exigence epinglee et sommee passe",
            check_requirements(req, build_ok) == 0,
        )
        build_bad = tmp / "build-bad.sh"
        build_bad.write_text("pip install -r r.txt\n", encoding="utf-8")
        control(
            "un pip install sans --require-hashes est un ecart",
            check_requirements(req, build_bad) > 0,
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
            "un telechargement de workflow non exempte est un ecart",
            check_workflows(wf, set()) > 0,
        )
        control(
            "le meme telechargement exempte nominalement passe",
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
            "une exemption perimee est un ecart",
            check_workflows(wf, {("fixture.yml", "Fetch tool")}) > 0,
        )

    print(f"\nselftest: {failures} controle(s) rate(s)")
    return 1 if failures else 0


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    problems = 0
    problems += check_llama(REPO_ROOT / LLAMA_FETCH, REPO_ROOT / LLAMA_SUMS)
    problems += check_python_standalone(
        REPO_ROOT / PBS_FETCH, REPO_ROOT / PBS_SUMS
    )
    problems += check_requirements(
        REPO_ROOT / REQUIREMENTS, REPO_ROOT / BUNDLE_BUILD
    )
    problems += check_workflows(REPO_ROOT / WORKFLOW_DIR, EXEMPT_WORKFLOW_STEPS)
    problems += check_fetch_scripts(REPO_ROOT, SCRIPT_DIRS)
    if "--github" in sys.argv:
        problems += check_github()
    print(f"\necarts={problems}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
