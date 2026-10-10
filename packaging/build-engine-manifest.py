#!/usr/bin/env python3
"""Build the signed engine manifest of one channel, with the archives it lists.

The application downloads some engines after install: the llama-server build
for NVIDIA cards, and the Node.js that runs tool servers. It takes them from a
channel (`stable` or `preview`) that publishes one manifest, `engines.json`,
and its minisign signature, `engines.json.minisig`, and it requests no archive
before that signature held for the distribution key compiled into it. This
script produces the three things a channel publishes, into one directory:

  engines.json           the manifest, in the format the engine store reads
  engines.json.minisig   its signature, by the distribution key
  <engine>-<tag>-<variant>-<platform>.zip
                         one archive per package, listed by the manifest with
                         its size and its SHA-256

What goes in is pinned in `packaging/engine-sources.json`: for each package,
the upstream archives and the SHA-256 of each. A source whose digest differs
from its pin stops the run before anything is written, so nothing unverified
is ever signed.

Every package is repackaged as one zip, the only format the engine store
reads. An upstream `.tar.gz` (macOS, Linux) becomes a zip that keeps the
permission bits and the links between sibling libraries. A package made of
several upstream archives (the Windows CUDA server and its runtime libraries)
becomes one zip. A symbolic link that climbs out of its own directory is left
out and named in the output: the store refuses such a link, and the three that
a Node.js release carries (`bin/npm`, `bin/npx`, `bin/corepack`) are
conveniences the application does not use, since it runs `npx-cli.js` by path.

Signing is `cargo tauri signer sign`, the tool that already signs the
application updates, with another key: the distribution key, read from
`APOLLIA_DIST_SIGNING_KEY` and `APOLLIA_DIST_SIGNING_KEY_PASSWORD`. It is
never read from a file of the repository and never passed on a command line.
The updater key (`TAURI_SIGNING_PRIVATE_KEY`) signs installers and nothing
here: a run where the two hold the same value is refused.

Usage:
    build-engine-manifest.py --channel stable --sequence 1760000000 \\
        --llama-tag b11541 --out dist/engines-stable
    build-engine-manifest.py ... --require llama-server/cuda/windows-x64
    build-engine-manifest.py ... --offline-dir downloads --no-sign

  --channel       `stable` or `preview`
  --sequence      strictly increasing per channel; the store refuses a lower
                  one than the last it accepted, so an old manifest cannot
                  bring an old engine back
  --llama-tag     the llama.cpp tag whose pinned packages are published; the
                  other engines are published at the one tag they are pinned at
  --require       `engine/variant/platform` that must be in the manifest, as
                  many times as needed; a channel that silently lost a package
                  is refused here rather than discovered on a user's machine
  --sources       the pin file (default: engine-sources.json beside this script)
  --offline-dir   take each source from this directory by its file name
                  instead of the network; the digests are checked all the same
  --cache-dir     where downloads are kept (default: <out>/.sources)
  --no-sign       write no signature (to inspect a manifest; never to publish)

Exit codes:
    0  the directory is complete
    1  refused: a digest differs, a package is missing, an archive is unusable,
       the key is absent, or signing failed
    2  called wrongly, or the pin file is unreadable
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath

MANIFEST_FORMAT = 1
MANIFEST_FILE = "engines.json"
SIGNATURE_FILE = "engines.json.minisig"
CHANNELS = ("stable", "preview")
ENGINES = ("llama-server", "vllm-mlx", "nodejs")
VARIANTS = ("cuda", "vulkan", "cpu", "metal")
PLATFORM = re.compile(r"^(macos|linux|windows)-(x64|arm64)$")
SHA256_HEX = re.compile(r"^[0-9a-f]{64}$")
SAFE_TAG = re.compile(r"^[A-Za-z0-9._-]{1,64}$")
CALENDAR_DAY = re.compile(r"^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$")

# The engine store refuses an archive with more entries than this.
MAX_ENTRIES = 20_000
# What each engine's archive must hold for the application to run it. An
# archive without it would pass every digest check and serve no one.
MUST_HOLD = {
    "llama-server": [("llama-server", "llama-server.exe")],
    "nodejs": [("node", "node.exe"), ("npx-cli.js",)],
}

DIST_KEY = "APOLLIA_DIST_SIGNING_KEY"
DIST_KEY_PASSWORD = "APOLLIA_DIST_SIGNING_KEY_PASSWORD"
UPDATER_KEY = "TAURI_SIGNING_PRIVATE_KEY"
UPDATER_KEY_PATH = "TAURI_SIGNING_PRIVATE_KEY_PATH"
UPDATER_KEY_PASSWORD = "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"


class Refused(Exception):
    """The run cannot produce a directory worth publishing."""


def say(message: str) -> None:
    print(message, flush=True)


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


# ── the pin file ────────────────────────────────────────────────────────────


def load_sources(path: Path) -> list[dict]:
    """The packages of the pin file, each checked for form."""
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as err:
        print(f"unreadable pin file {path}: {err}", file=sys.stderr)
        sys.exit(2)
    packages = document.get("packages")
    if not isinstance(packages, list) or not packages:
        print(f"{path}: no package is pinned", file=sys.stderr)
        sys.exit(2)
    for package in packages:
        where = f"{path}: {package.get('engine')} {package.get('tag')} {package.get('platform')}"
        if package.get("engine") not in ENGINES:
            raise Refused(f"{where}: unknown engine")
        if package.get("variant") not in VARIANTS:
            raise Refused(f"{where}: unknown variant {package.get('variant')!r}")
        if not PLATFORM.match(str(package.get("platform", ""))):
            raise Refused(f"{where}: platform is not <os>-<arch>")
        if not SAFE_TAG.match(str(package.get("tag", ""))):
            raise Refused(f"{where}: unsafe tag")
        released = package.get("released")
        if released is not None and not CALENDAR_DAY.match(str(released)):
            raise Refused(f"{where}: released {released!r} is not YYYY-MM-DD")
        sources = package.get("sources")
        if not isinstance(sources, list) or not sources:
            raise Refused(f"{where}: no source")
        for source in sources:
            url = str(source.get("url", ""))
            if urllib.parse.urlparse(url).scheme != "https":
                raise Refused(f"{where}: source {url!r} is not an https address")
            if not SHA256_HEX.match(str(source.get("sha256", ""))):
                raise Refused(f"{where}: source {url} carries no 64-hex sha256")
            if not url.endswith((".zip", ".tar.gz", ".tgz")):
                raise Refused(f"{where}: source {url} is neither a zip nor a tar.gz")
    return packages


def select(packages: list[dict], llama_tag: str) -> list[dict]:
    """The packages one publication carries, one per engine, variant and platform."""
    chosen = [
        p for p in packages if p["engine"] != "llama-server" or p["tag"] == llama_tag
    ]
    seen: dict[tuple[str, str, str], str] = {}
    for package in chosen:
        key = (package["engine"], package["variant"], package["platform"])
        if key in seen:
            raise Refused(
                f"{'/'.join(key)} is pinned twice ({seen[key]} and {package['tag']}): "
                "a channel offers one build of it"
            )
        seen[key] = package["tag"]
    return chosen


def check_required(chosen: list[dict], required: list[str], llama_tag: str) -> None:
    have = {f"{p['engine']}/{p['variant']}/{p['platform']}" for p in chosen}
    missing = [r for r in required if r not in have]
    if missing:
        raise Refused(
            "the channel would be published without "
            + ", ".join(missing)
            + f". Pin it in packaging/engine-sources.json (llama-server at tag {llama_tag}) "
            "or publish a tag where it is pinned"
        )


# ── sources ─────────────────────────────────────────────────────────────────


def source_name(url: str) -> str:
    return PurePosixPath(urllib.parse.urlparse(url).path).name


def download(url: str, dest: Path) -> None:
    last: Exception | None = None
    for attempt in (1, 2, 3):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": "apollia-engine-channel"})
            with urllib.request.urlopen(request, timeout=120) as response:
                partial = dest.with_name(dest.name + ".part")
                with partial.open("wb") as out:
                    shutil.copyfileobj(response, out, 1 << 20)
                partial.replace(dest)
            return
        except (urllib.error.URLError, OSError) as err:
            last = err
            say(f"   download failed (attempt {attempt}/3): {err}")
            time.sleep(attempt * 5)
    raise Refused(f"could not download {url}: {last}")


def obtain(source: dict, cache_dir: Path, offline_dir: Path | None) -> Path:
    """The local file of a source, its digest equal to the pin."""
    url, pinned = source["url"], source["sha256"]
    name = source_name(url)
    if offline_dir is not None:
        path = offline_dir / name
        if not path.is_file():
            raise Refused(f"{name} is not in {offline_dir}")
    else:
        cache_dir.mkdir(parents=True, exist_ok=True)
        path = cache_dir / name
        if not (path.is_file() and sha256_of(path) == pinned):
            say(f"   downloading {url}")
            download(url, path)
    actual = sha256_of(path)
    if actual != pinned:
        raise Refused(
            f"{name}: sha256 {actual} differs from the pinned {pinned}. Nothing was signed"
        )
    say(f"   verified {name}")
    return path


# ── repackaging ─────────────────────────────────────────────────────────────


class Entry:
    """One member of an upstream archive, whatever its format."""

    def __init__(self, name, kind, mode, mtime, size, opener=None, link=None):
        self.name = name  # as upstream names it
        self.kind = kind  # "dir", "file" or "link"
        self.mode = mode  # permission bits
        self.mtime = mtime  # (Y, M, D, h, m, s)
        self.size = size
        self.opener = opener  # () -> binary reader, for a file
        self.link = link  # the target, for a link


def clamp_time(stamp: tuple) -> tuple:
    year = min(max(stamp[0], 1980), 2107)
    return (year, *stamp[1:6])


def zip_entries(archive: zipfile.ZipFile):
    for info in archive.infolist():
        mode = info.external_attr >> 16 if info.create_system == 3 else 0
        stamp = clamp_time(info.date_time)
        if info.is_dir():
            yield Entry(info.filename, "dir", 0o755, stamp, 0)
        elif stat.S_ISLNK(mode):
            target = archive.read(info).decode("utf-8")
            yield Entry(info.filename, "link", 0o777, stamp, 0, link=target)
        else:
            perms = stat.S_IMODE(mode) or 0o644
            yield Entry(
                info.filename, "file", perms, stamp, info.file_size,
                opener=lambda info=info: archive.open(info),
            )


def tar_entries(archive: tarfile.TarFile):
    for member in archive:
        stamp = clamp_time(time.gmtime(member.mtime)[:6])
        if member.isdir():
            yield Entry(member.name, "dir", 0o755, stamp, 0)
        elif member.issym():
            yield Entry(member.name, "link", 0o777, stamp, 0, link=member.linkname)
        elif member.isreg() or member.islnk():
            # A hard link is written as the file it names: a zip has none.
            target = archive.getmember(member.linkname) if member.islnk() else member
            yield Entry(
                member.name, "file", stat.S_IMODE(member.mode) or 0o644, stamp, target.size,
                opener=lambda target=target: archive.extractfile(target),
            )
        else:
            raise Refused(f"{member.name}: a device or a pipe has no place in an engine archive")


def open_source(path: Path):
    """The archive of a source and the iterator over its entries."""
    if path.name.endswith(".zip"):
        archive = zipfile.ZipFile(path)
        return archive, zip_entries(archive)
    archive = tarfile.open(path, "r:gz")
    return archive, tar_entries(archive)


def clean_name(name: str) -> str:
    """An entry name the store accepts, or a refusal."""
    if "\\" in name:
        raise Refused(f"entry {name!r} carries a backslash")
    parts = [p for p in name.split("/") if p not in ("", ".")]
    if name.startswith("/") or ".." in parts or not parts:
        raise Refused(f"entry {name!r} leaves the archive root")
    return "/".join(parts)


def link_stays_inside(target: str) -> bool:
    """The rule of the engine store: a link points beside or below itself."""
    return (
        bool(target)
        and not target.startswith("/")
        and "\\" not in target
        and ":" not in target
        and ".." not in target.split("/")
    )


def write_entry(out: zipfile.ZipFile, name: str, entry: Entry) -> None:
    if entry.kind == "dir":
        info = zipfile.ZipInfo(name + "/", entry.mtime)
        info.create_system = 3
        info.external_attr = ((stat.S_IFDIR | 0o755) << 16) | 0x10
        out.writestr(info, b"")
    elif entry.kind == "link":
        info = zipfile.ZipInfo(name, entry.mtime)
        info.create_system = 3
        info.external_attr = (stat.S_IFLNK | 0o777) << 16
        out.writestr(info, entry.link.encode("utf-8"))
    else:
        info = zipfile.ZipInfo(name, entry.mtime)
        info.create_system = 3
        info.external_attr = (stat.S_IFREG | entry.mode) << 16
        info.compress_type = zipfile.ZIP_DEFLATED
        with entry.opener() as reader, out.open(
            info, "w", force_zip64=entry.size >= 0x7FFF_FFFF
        ) as writer:
            shutil.copyfileobj(reader, writer, 1 << 20)


def repackage(package: dict, files: list[Path], dest: Path) -> None:
    """Write the one zip of a package from its verified upstream archives."""
    written: dict[str, str] = {}  # name -> kind
    dropped: list[str] = []
    with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as out:
        for source, path in zip(package["sources"], files):
            beside = source.get("beside")
            anchor = None
            if beside is not None:
                holders = [n for n, k in written.items() if k == "file" and PurePosixPath(n).name == beside]
                if not holders:
                    raise Refused(
                        f"{path.name} goes beside {beside}, which no earlier source of the package holds"
                    )
                anchor = str(PurePosixPath(holders[0]).parent)
            archive, entries = open_source(path)
            with archive:
                for entry in entries:
                    name = clean_name(entry.name)
                    if anchor is not None:
                        if entry.kind == "dir":
                            continue
                        leaf = PurePosixPath(name).name
                        name = leaf if anchor == "." else f"{anchor}/{leaf}"
                    if entry.kind == "link" and not link_stays_inside(entry.link):
                        dropped.append(f"{name} -> {entry.link}")
                        continue
                    if name in written:
                        if entry.kind == "dir" and written[name] == "dir":
                            continue
                        raise Refused(f"{dest.name}: {name} comes from two sources")
                    write_entry(out, name, entry)
                    written[name] = entry.kind
    for line in dropped:
        say(f"   left out a link that climbs out of its directory: {line}")
    if len(written) > MAX_ENTRIES:
        raise Refused(f"{dest.name}: {len(written)} entries, the store accepts {MAX_ENTRIES}")
    leaves = {PurePosixPath(n).name for n, k in written.items() if k == "file"}
    for alternatives in MUST_HOLD.get(package["engine"], []):
        if not leaves.intersection(alternatives):
            raise Refused(f"{dest.name} holds no {' or '.join(alternatives)}")


# ── manifest and signature ──────────────────────────────────────────────────


def build_manifest(channel: str, sequence: int, built: list[tuple[dict, Path]]) -> bytes:
    packages = []
    for package, archive in built:
        record = {
            "engine": package["engine"],
            "tag": package["tag"],
            "variant": package["variant"],
            "platform": package["platform"],
            "size": archive.stat().st_size,
            "url": archive.name,
            "sha256": sha256_of(archive),
        }
        if package.get("released"):
            record["released"] = package["released"]
        packages.append(record)
    manifest = {
        "format": MANIFEST_FORMAT,
        "channel": channel,
        "sequence": sequence,
        "packages": packages,
    }
    return (json.dumps(manifest, indent=2) + "\n").encode("utf-8")


def sign(manifest: Path) -> Path:
    """Sign `manifest` with the distribution key and return the signature path."""
    key = os.environ.get(DIST_KEY, "").strip()
    if not key:
        raise Refused(
            f"{DIST_KEY} is not set: the manifest cannot be signed. The key is read "
            "from the environment only, never from a file of the repository"
        )
    if key == os.environ.get(UPDATER_KEY, "").strip():
        raise Refused(
            f"{DIST_KEY} holds the same key as {UPDATER_KEY}. The distribution key "
            "is a dedicated one: the updater key signs installers and nothing else"
        )
    env = {k: v for k, v in os.environ.items() if k not in (UPDATER_KEY_PATH, DIST_KEY, DIST_KEY_PASSWORD)}
    # The signer reads the key under these names. They are set for this one
    # child process, so the key never appears in a command line.
    env[UPDATER_KEY] = key
    env[UPDATER_KEY_PASSWORD] = os.environ.get(DIST_KEY_PASSWORD, "")
    try:
        done = subprocess.run(
            ["cargo", "tauri", "signer", "sign", str(manifest)],
            env=env,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError as err:
        raise Refused(f"`cargo tauri signer sign` could not be run: {err}") from err
    produced = manifest.with_name(manifest.name + ".sig")
    if done.returncode != 0 or not produced.is_file():
        detail = (done.stderr or done.stdout).strip().splitlines()[-1:] or ["no output"]
        raise Refused(
            "`cargo tauri signer sign` failed (is tauri-cli installed, and does "
            f"{DIST_KEY_PASSWORD} open the key?): {detail[0]}"
        )
    signature = manifest.with_name(SIGNATURE_FILE)
    produced.replace(signature)
    return signature


# ── entry point ─────────────────────────────────────────────────────────────


def run(args: argparse.Namespace) -> None:
    out_dir = Path(args.out)
    if out_dir.exists() and any(p.name != ".sources" for p in out_dir.iterdir()):
        raise Refused(f"{out_dir} is not empty: a publication starts from nothing")
    packages = load_sources(Path(args.sources))
    chosen = select(packages, args.llama_tag)
    check_required(chosen, args.require, args.llama_tag)
    if not chosen:
        raise Refused("no package to publish")
    out_dir.mkdir(parents=True, exist_ok=True)
    cache_dir = Path(args.cache_dir) if args.cache_dir else out_dir / ".sources"
    offline_dir = Path(args.offline_dir) if args.offline_dir else None

    # Every source of every package is verified before the first archive is
    # written: a digest that differs leaves no archive behind.
    verified: list[tuple[dict, list[Path]]] = []
    for package in chosen:
        say(f"== {package['engine']} {package['tag']} {package['variant']} {package['platform']}")
        verified.append((package, [obtain(s, cache_dir, offline_dir) for s in package["sources"]]))

    built: list[tuple[dict, Path]] = []
    for package, files in verified:
        name = f"{package['engine']}-{package['tag']}-{package['variant']}-{package['platform']}.zip"
        say(f"== repackaging {name}")
        dest = out_dir / name
        repackage(package, files, dest)
        built.append((package, dest))

    manifest = out_dir / MANIFEST_FILE
    manifest.write_bytes(build_manifest(args.channel, args.sequence, built))
    say(f"== wrote {manifest} (channel {args.channel}, sequence {args.sequence}, {len(built)} package(s))")
    if args.no_sign:
        say("== not signed (--no-sign): this directory must not be published")
    else:
        say(f"== signed: {sign(manifest)}")
    if cache_dir == out_dir / ".sources" and cache_dir.is_dir():
        shutil.rmtree(cache_dir)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Build the signed engine manifest of one channel, with its archives."
    )
    parser.add_argument("--channel", required=True, choices=CHANNELS)
    parser.add_argument("--sequence", required=True, type=int)
    parser.add_argument("--llama-tag", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--require", action="append", default=[])
    parser.add_argument(
        "--sources", default=str(Path(__file__).resolve().parent / "engine-sources.json")
    )
    parser.add_argument("--offline-dir")
    parser.add_argument("--cache-dir")
    parser.add_argument("--no-sign", action="store_true")
    args = parser.parse_args()
    if args.sequence < 1:
        parser.error("--sequence starts at 1")
    if not SAFE_TAG.match(args.llama_tag):
        parser.error("--llama-tag is not a tag")
    for required in args.require:
        if len(required.split("/")) != 3:
            parser.error(f"--require {required!r} is not engine/variant/platform")
    try:
        run(args)
    except Refused as refusal:
        print(f"refused: {refusal}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
