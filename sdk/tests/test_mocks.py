"""Tests for apollia.testing.mocks - low-level mock proxies."""

import pytest

from apollia.testing import MockContext, MockLlmProxy, MockMemory, MockToolProxy


@pytest.mark.asyncio
async def test_mock_context_wires_all_surfaces():
    """The factory-built MockContext exposes every ctx.* surface."""
    ctx = MockContext()
    # Core surfaces wired by the factory:
    assert ctx.tools is not None
    assert ctx.llm is not None
    assert ctx.memory is not None
    assert ctx.a2a is not None
    assert ctx.datasources is not None
    assert ctx.templates is not None
    assert ctx.secrets is not None
    assert ctx.events is not None
    assert ctx.profile is not None
    assert ctx.workspace is not None
    assert ctx.stt is not None
    assert ctx.notify is not None
    assert ctx.budget is not None
    assert ctx.logger is not None


@pytest.mark.asyncio
async def test_mock_context_surfaces_are_isolated_per_instance():
    """Each MockContext() yields fresh, independent surface mocks."""
    ctx1 = MockContext()
    ctx2 = MockContext()
    assert ctx1.tools is not ctx2.tools
    assert ctx1.llm is not ctx2.llm
    assert ctx1.events is not ctx2.events


@pytest.mark.asyncio
async def test_mock_tool_proxy_records_calls():
    """MockToolProxy records each call and returns the configured response."""
    proxy = MockToolProxy({"bash": {"output": "ok", "exit_code": 0}})

    result = await proxy.call("bash", {"cmd": "ls"})

    assert result == {"output": "ok", "exit_code": 0}
    assert proxy.calls == [("bash", {"cmd": "ls"})]
    assert proxy.tool_call_count() == 1


@pytest.mark.asyncio
async def test_mock_tool_proxy_assert_called_with():
    """assert_called_with passes for matching calls and fails otherwise."""
    proxy = MockToolProxy({"bash": {"output": "ok"}})
    await proxy.call("bash", {"cmd": "ls"})

    proxy.assert_called("bash")
    proxy.assert_called_with("bash", {"cmd": "ls"})

    with pytest.raises(AssertionError):
        proxy.assert_called("python")

    with pytest.raises(AssertionError):
        proxy.assert_called_with("bash", {"cmd": "pwd"})


@pytest.mark.asyncio
async def test_mock_tool_proxy_unknown_tool_raises():
    """Calling an unconfigured tool raises KeyError."""
    proxy = MockToolProxy({"bash": {"output": "ok"}})

    with pytest.raises(KeyError, match="python"):
        await proxy.call("python", {})


@pytest.mark.asyncio
async def test_mock_tool_proxy_list_and_describe():
    """list_tools and describe expose configured tool metadata."""
    proxy = MockToolProxy({"bash": {"output": "ok"}, "python": {"output": "hi"}})

    assert sorted(proxy.list_tools()) == ["bash", "python"]

    desc = await proxy.describe("bash")
    assert desc is not None
    assert desc["name"] == "bash"

    assert await proxy.describe("unknown") is None


@pytest.mark.asyncio
async def test_mock_llm_proxy_sequential_responses():
    """MockLlmProxy returns responses in FIFO order then raises on exhaustion."""
    proxy = MockLlmProxy([{"content": "first"}, {"content": "second"}])

    r1 = await proxy.complete("prompt1")
    r2 = await proxy.complete("prompt2")

    # complete() returns a MockLlmResponse, accessed via .content or [].
    assert r1.content == "first"
    assert r2.content == "second"
    assert r1["content"] == "first"
    proxy.assert_called_count(2)

    with pytest.raises(IndexError):
        await proxy.complete("prompt3")


@pytest.mark.asyncio
async def test_mock_llm_proxy_chat():
    """MockLlmProxy.chat() delegates to complete() and records the call."""
    proxy = MockLlmProxy([{"content": "reply"}])

    result = await proxy.chat(system="You are helpful.", user="Hello")

    assert result.content == "reply"
    assert proxy.call_count == 1
    assert len(proxy.prompts) == 1
    assert proxy.default_backend == "mock"


@pytest.mark.asyncio
async def test_mock_llm_proxy_assert_called_count_failure():
    """assert_called_count raises when actual differs from expected."""
    proxy = MockLlmProxy([{"content": "a"}])
    await proxy.complete("p")

    with pytest.raises(AssertionError, match="Expected 5"):
        proxy.assert_called_count(5)


@pytest.mark.asyncio
async def test_mock_memory_record_and_recall():
    """MockMemory stores episodic events and semantic key/values."""
    mem = MockMemory()

    await mem.record("user logged in", importance=0.8)
    await mem.remember("user_name", "alice")
    result = await mem.recall("user_name")

    assert result == "alice"
    assert len(mem.episodes) == 1
    assert mem.episodes[0]["content"] == "user logged in"
    assert len(mem.operations) == 3


@pytest.mark.asyncio
async def test_mock_memory_recall_missing_key():
    """Recalling a non-existent key returns None."""
    mem = MockMemory()
    result = await mem.recall("missing")
    assert result is None


@pytest.mark.asyncio
async def test_mock_memory_forget():
    """forget() removes the entry and records the operation."""
    mem = MockMemory()
    await mem.remember("key1", "value1")
    await mem.forget("key1")

    assert await mem.recall("key1") is None
    forget_op = [op for op in mem.operations if op["op"] == "forget"][0]
    assert forget_op["existed"] is True


@pytest.mark.asyncio
async def test_mock_memory_forget_nonexistent():
    """forget() on a missing key records existed=False."""
    mem = MockMemory()
    await mem.forget("nope")

    forget_op = [op for op in mem.operations if op["op"] == "forget"][0]
    assert forget_op["existed"] is False


@pytest.mark.asyncio
async def test_mock_memory_search():
    """search() returns entries whose key contains the query string."""
    mem = MockMemory()
    await mem.remember("user_name", "alice")
    await mem.remember("user_email", "alice@example.com")
    await mem.remember("system_version", "1.0")

    results = await mem.search("user")

    assert len(results) == 2
    assert all(r["source"] == "mock" for r in results)
