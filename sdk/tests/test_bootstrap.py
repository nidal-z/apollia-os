"""Tests for apollia.bootstrap — ContextBootstrap state machine and guards."""

import json

import pytest

from apollia.bootstrap import ContextBootstrap
from apollia.testing.mocks import MockMemory


# --- Concrete test implementation ---


class StubBootstrap(ContextBootstrap):
    """Minimal implementation for testing the base class infrastructure."""

    def __init__(self, *, stale: bool = False) -> None:
        self._stale = stale
        self.bootstrap_calls: int = 0

    async def is_stale(self, ctx: object) -> bool:
        return self._stale

    async def run_bootstrap(self, ctx: object) -> dict:
        self.bootstrap_calls += 1
        snapshot = {"bootstrapped": True}
        await self.persist(ctx, snapshot, staleness_marker="test-marker")
        return snapshot


class FakeCtx:
    """Minimal context with optional MockMemory."""

    def __init__(self, *, memory: MockMemory | None = None) -> None:
        self.memory = memory


# --- needs_bootstrap ---


@pytest.mark.asyncio
async def test_needs_bootstrap_returns_true_when_no_memory() -> None:
    """GIVEN ctx.memory is None WHEN needs_bootstrap THEN returns True."""
    ctx = FakeCtx(memory=None)
    bootstrap = StubBootstrap()
    assert await bootstrap.needs_bootstrap(ctx) is True


@pytest.mark.asyncio
async def test_needs_bootstrap_returns_true_when_no_status() -> None:
    """GIVEN no status key in memory WHEN needs_bootstrap THEN returns True."""
    ctx = FakeCtx(memory=MockMemory())
    bootstrap = StubBootstrap()
    assert await bootstrap.needs_bootstrap(ctx) is True


@pytest.mark.asyncio
async def test_needs_bootstrap_returns_true_when_status_missing() -> None:
    """GIVEN status is 'missing' WHEN needs_bootstrap THEN returns True."""
    mem = MockMemory()
    await mem.remember("bootstrap.status", "missing")
    ctx = FakeCtx(memory=mem)
    bootstrap = StubBootstrap()
    assert await bootstrap.needs_bootstrap(ctx) is True


@pytest.mark.asyncio
async def test_needs_bootstrap_returns_true_when_status_partial() -> None:
    """GIVEN status is 'partial' WHEN needs_bootstrap THEN returns True."""
    mem = MockMemory()
    await mem.remember("bootstrap.status", "partial")
    ctx = FakeCtx(memory=mem)
    bootstrap = StubBootstrap()
    assert await bootstrap.needs_bootstrap(ctx) is True


@pytest.mark.asyncio
async def test_needs_bootstrap_delegates_to_is_stale_when_complete() -> None:
    """GIVEN status is 'complete' WHEN needs_bootstrap THEN delegates to is_stale()."""
    mem = MockMemory()
    await mem.remember("bootstrap.status", "complete")
    ctx = FakeCtx(memory=mem)

    # WHEN is_stale returns False
    bootstrap_fresh = StubBootstrap(stale=False)
    assert await bootstrap_fresh.needs_bootstrap(ctx) is False

    # WHEN is_stale returns True
    bootstrap_stale = StubBootstrap(stale=True)
    assert await bootstrap_stale.needs_bootstrap(ctx) is True


# --- persist guard ---


@pytest.mark.asyncio
async def test_persist_guard_does_not_overwrite_complete_with_partial() -> None:
    """GIVEN status is 'complete' WHEN persist(status='partial') THEN snapshot is unchanged."""
    mem = MockMemory()
    ctx = FakeCtx(memory=mem)

    # First: persist a complete snapshot
    bootstrap = StubBootstrap()
    await bootstrap.persist(
        ctx,
        {"version": 1},
        staleness_marker="v1",
        status="complete",
    )
    assert mem.store["bootstrap.status"] == "complete"
    assert json.loads(mem.store["bootstrap.snapshot"]) == {"version": 1}

    # Then: try to persist a partial snapshot — should be rejected
    await bootstrap.persist(
        ctx,
        {"version": 2, "partial": True},
        staleness_marker="v2",
        status="partial",
    )

    # Status and snapshot must remain unchanged
    assert mem.store["bootstrap.status"] == "complete"
    assert json.loads(mem.store["bootstrap.snapshot"]) == {"version": 1}


@pytest.mark.asyncio
async def test_persist_allows_complete_to_overwrite_complete() -> None:
    """GIVEN status is 'complete' WHEN persist(status='complete') THEN snapshot is updated."""
    mem = MockMemory()
    ctx = FakeCtx(memory=mem)

    bootstrap = StubBootstrap()
    await bootstrap.persist(ctx, {"v": 1}, staleness_marker="a", status="complete")
    await bootstrap.persist(ctx, {"v": 2}, staleness_marker="b", status="complete")

    assert json.loads(mem.store["bootstrap.snapshot"]) == {"v": 2}
    meta = json.loads(mem.store["bootstrap.meta"])
    assert meta["staleness_marker"] == "b"


@pytest.mark.asyncio
async def test_persist_allows_partial_when_no_existing_status() -> None:
    """GIVEN no existing status WHEN persist(status='partial') THEN snapshot is written."""
    mem = MockMemory()
    ctx = FakeCtx(memory=mem)

    bootstrap = StubBootstrap()
    await bootstrap.persist(ctx, {"partial": True}, staleness_marker="p1", status="partial")

    assert mem.store["bootstrap.status"] == "partial"
    assert json.loads(mem.store["bootstrap.snapshot"]) == {"partial": True}


# --- load_snapshot ---


@pytest.mark.asyncio
async def test_load_snapshot_returns_none_when_no_memory() -> None:
    """GIVEN ctx.memory is None WHEN load_snapshot THEN returns None."""
    ctx = FakeCtx(memory=None)
    bootstrap = StubBootstrap()
    assert await bootstrap.load_snapshot(ctx) is None


@pytest.mark.asyncio
async def test_load_snapshot_returns_none_when_no_key() -> None:
    """GIVEN empty memory WHEN load_snapshot THEN returns None."""
    ctx = FakeCtx(memory=MockMemory())
    bootstrap = StubBootstrap()
    assert await bootstrap.load_snapshot(ctx) is None


@pytest.mark.asyncio
async def test_load_snapshot_returns_parsed_json() -> None:
    """GIVEN snapshot stored as JSON WHEN load_snapshot THEN returns parsed dict."""
    mem = MockMemory()
    await mem.remember("bootstrap.snapshot", json.dumps({"key": "value"}))
    ctx = FakeCtx(memory=mem)

    bootstrap = StubBootstrap()
    snapshot = await bootstrap.load_snapshot(ctx)
    assert snapshot == {"key": "value"}


@pytest.mark.asyncio
async def test_load_snapshot_returns_none_on_invalid_json() -> None:
    """GIVEN corrupt JSON in snapshot key WHEN load_snapshot THEN returns None."""
    mem = MockMemory()
    await mem.remember("bootstrap.snapshot", "not-json{{{")
    ctx = FakeCtx(memory=mem)

    bootstrap = StubBootstrap()
    assert await bootstrap.load_snapshot(ctx) is None


# --- load_meta ---


@pytest.mark.asyncio
async def test_load_meta_returns_parsed_metadata() -> None:
    """GIVEN meta stored as JSON WHEN load_meta THEN returns parsed dict."""
    mem = MockMemory()
    meta = {"version": 1, "staleness_marker": "abc", "created_at": "2026-04-15T00:00:00Z"}
    await mem.remember("bootstrap.meta", json.dumps(meta))
    ctx = FakeCtx(memory=mem)

    bootstrap = StubBootstrap()
    result = await bootstrap.load_meta(ctx)
    assert result == meta


# --- run_bootstrap integration ---


@pytest.mark.asyncio
async def test_run_bootstrap_persists_all_three_keys() -> None:
    """GIVEN fresh memory WHEN run_bootstrap THEN snapshot, meta, status are all written."""
    mem = MockMemory()
    ctx = FakeCtx(memory=mem)

    bootstrap = StubBootstrap()
    snapshot = await bootstrap.run_bootstrap(ctx)

    assert snapshot == {"bootstrapped": True}
    assert mem.store["bootstrap.status"] == "complete"
    assert json.loads(mem.store["bootstrap.snapshot"]) == {"bootstrapped": True}

    meta = json.loads(mem.store["bootstrap.meta"])
    assert meta["version"] == 1
    assert meta["staleness_marker"] == "test-marker"
    assert "created_at" in meta


@pytest.mark.asyncio
async def test_persist_is_noop_when_no_memory() -> None:
    """GIVEN ctx.memory is None WHEN persist THEN no error, no effect."""
    ctx = FakeCtx(memory=None)
    bootstrap = StubBootstrap()
    # Should not raise
    await bootstrap.persist(ctx, {"x": 1}, staleness_marker="y")
