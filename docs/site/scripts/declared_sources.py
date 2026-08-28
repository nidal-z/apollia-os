#!/usr/bin/env python3
"""The sources a generator of this directory declares, and the crossing on them.

Every generator here reads Rust or Python source and splices the result into a
published page. Each one therefore points at a file and at a symbol inside it,
and both pointers are written in the generator rather than checked anywhere.

That went wrong exactly the way an unchecked pointer goes wrong.
`gen_config_ref.py` looked for `LlmConfig` in `crates/apollia-llm/src/router.rs`;
a module split moved the struct to `router/config.rs`; the generator printed
`warning: struct LlmConfig not found`, exited **0**, and `docs/site/regen.sh`
deleted the whole `### [llm]` table from the published configuration reference
without a single red anywhere.

Two things close that. This module is the first: a generator declares its
sources as `Source` values and calls `require()` before it writes anything, so a
pointer that no longer resolves stops the run instead of producing an amputated
page. The second is `scripts/check_doc_generators.py`, which reads the same
declarations and crosses them with the tree, so the break is caught at the
commit that moves the file rather than at the next regeneration.

Exit codes follow the corpus rule that a tool which measured nothing answers a
code distinct from one that measured a failure. A declared source that is absent
from the tree, or a symbol the file no longer holds, means the generator has no
subject to read: it measured nothing, so `require()` yields **2**. A defect
found inside a source it did read stays the generator's own **1**.
"""

import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]


@dataclass(frozen=True)
class Source:
    """One pointer a generator declares, and what has to be true of it.

    `path` is repo-relative. `symbol` is the literal declaration line the
    generator looks for, `pub struct LlmConfig` or `fn categorize(`, written as
    the generator writes it so the two cannot drift apart. A directory source
    leaves `symbol` at `None` and is required to hold at least one entry.
    """

    path: str
    symbol: str | None = None
    why: str = ""

    def unresolved(self, root: Path = REPO_ROOT) -> str | None:
        """The reason this pointer does not resolve, or `None` when it does.

        `root` is a parameter so `scripts/check_doc_generators.py` can exercise
        both directions on a temporary fixture rather than on the tree.
        """
        full = root / self.path
        if not full.exists():
            return f"{self.path} does not exist"
        if full.is_dir():
            if self.symbol is not None:
                return f"{self.path} is a directory, so `{self.symbol}` cannot be read from it"
            if not any(full.iterdir()):
                return f"{self.path} is an empty directory"
            return None
        if self.symbol is None:
            return None
        text = full.read_text(encoding="utf-8", errors="replace")
        if self.symbol not in text:
            return f"`{self.symbol}` is not in {self.path}"
        return None


def unresolved(sources: list[Source], root: Path = REPO_ROOT) -> list[str]:
    """Every declared source that no longer resolves, in declaration order."""
    return [reason for s in sources if (reason := s.unresolved(root)) is not None]


def require(generator: str, sources: list[Source], root: Path = REPO_ROOT) -> int | None:
    """`None` when every declared source resolves, otherwise the code to return.

    The caller returns that code straight away and writes no page. Leaving the
    last coherent page on disk is the same choice `gen_sdk_ref.py` makes on a
    contract divergence, and for the same reason: an amputated page reads as a
    normal one.
    """
    if not sources:
        print(
            f"{generator}: NOTHING MEASURED, no source declared, so there is "
            "nothing to read. No page written.",
            file=sys.stderr,
        )
        return 2
    broken = unresolved(sources, root)
    if not broken:
        return None
    print(
        f"{generator}: NOTHING MEASURED, {len(broken)} of {len(sources)} declared "
        "source(s) no longer resolve. No page written.",
        file=sys.stderr,
    )
    for reason in broken:
        print(f"  {reason}", file=sys.stderr)
    print(
        "A generator that cannot find its subject publishes a page with the "
        "subject missing, which reads exactly like a page that has no subject. "
        "Repair the pointer in the generator, or move the symbol back.",
        file=sys.stderr,
    )
    return 2
