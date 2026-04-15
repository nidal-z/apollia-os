"""Base bootstrap class for development-domain agents.

Implements common discovery scopes shared by spec-assistant,
dev-assistant, and review-assistant.

Subclasses override ``extra_scopes()`` to add domain-specific discovery
(e.g. existing specs for spec-assistant, architecture modules for
dev-assistant).

Common snapshot structure::

    {
        "commit_hash": str,       # HEAD commit hash or "no-git"
        "workspace_rules": str,   # from ctx.workspace.rules
        "tech_stack": list[str],  # detected marker files
        "has_git": bool,          # True when inside a git repo
        **extra                   # populated by subclass via extra_scopes()
    }

Staleness: stale when HEAD commit hash differs from stored marker.
Non-git workspaces use ``"no-git"`` as a stable marker that never
triggers re-bootstrap.
"""

from __future__ import annotations

from typing import Any

from apollia.bootstrap import ContextBootstrap


# Marker files whose presence indicates a tech-stack language or tool.
_TECH_MARKERS: tuple[str, ...] = (
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
    "build.gradle",
)


class ProjectContextBootstrap(ContextBootstrap):
    """ContextBootstrap for development-domain agents.

    Discovers common project context (git state, workspace rules,
    tech stack) and persists it as a semantic memory snapshot.

    Staleness is determined by comparing the current ``HEAD`` commit
    hash against the marker stored in the previous bootstrap.  Non-git
    workspaces use ``"no-git"`` as a stable marker.

    Subclasses can extend the snapshot by overriding ``extra_scopes()``.
    """

    async def is_stale(self, ctx: Any) -> bool:
        """Return ``True`` when ``HEAD`` commit differs from stored marker."""
        meta = await self.load_meta(ctx)
        if meta is None:
            return True
        stored = meta.get("staleness_marker", "")
        current = await self._current_commit(ctx)
        return current != stored

    async def run_bootstrap(self, ctx: Any) -> dict[str, Any]:
        """Discover common project context and persist the snapshot.

        Steps:
        1. Resolve current commit hash (staleness marker).
        2. Capture workspace rules from ``ctx.workspace``.
        3. Detect tech-stack markers via ``file_read``.
        4. Call ``extra_scopes()`` for subclass-specific discovery.
        5. Persist everything to semantic memory.
        """
        snapshot: dict[str, Any] = {}

        # 1. Staleness marker
        commit = await self._current_commit(ctx)
        snapshot["commit_hash"] = commit
        snapshot["has_git"] = commit != "no-git"

        # 2. Workspace rules (via ctx.workspace -- no file_read needed)
        if ctx.workspace is not None:
            snapshot["workspace_rules"] = ctx.workspace.rules or ""
        else:
            snapshot["workspace_rules"] = ""

        # 3. Tech stack markers
        tech: list[str] = []
        if ctx.tools is not None:
            for marker in _TECH_MARKERS:
                result = await ctx.tools.call("file_read", {"path": marker})
                if result is not None and not result.get("error"):
                    tech.append(marker)
        snapshot["tech_stack"] = tech

        # 4. Subclass-specific scopes
        extra = await self.extra_scopes(ctx, snapshot)
        snapshot.update(extra)

        await self.persist(
            ctx,
            snapshot,
            staleness_marker=commit,
            source="bootstrap:project",
        )
        return snapshot

    async def extra_scopes(
        self,
        ctx: Any,
        base_snapshot: dict[str, Any],
    ) -> dict[str, Any]:
        """Hook for subclass-specific discovery.

        Called after common scopes are populated.  The returned dict is
        merged into the snapshot before persistence.

        Override in subclasses to add domain-specific keys (e.g.
        ``existing_specs``, ``architecture``, ``recent_files``).

        Parameters
        ----------
        ctx
            RuntimeContext with tools, memory, workspace, etc.
        base_snapshot
            The partially-built snapshot (commit_hash, workspace_rules,
            tech_stack, has_git already populated).

        Returns
        -------
        dict[str, Any]
            Extra keys to merge. Default implementation returns ``{}``.
        """
        return {}

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    async def _current_commit(self, ctx: Any) -> str:
        """Return the current ``HEAD`` commit hash, or ``"no-git"``."""
        if ctx.tools is None:
            return "no-git"
        result = await ctx.tools.call(
            "bash_executor",
            {"command": "git rev-parse HEAD 2>/dev/null || echo 'no-git'"},
        )
        if result is None:
            return "no-git"
        raw = result.get("stdout", "no-git")
        if not isinstance(raw, str):
            return "no-git"
        return raw.strip() or "no-git"
