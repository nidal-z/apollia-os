"""Tests for apollia.testing.mocks - low-level mock proxies."""

import pytest
from apollia.testing import MockContext, MockLlmProxy, MockMemory, MockToolProxy


@pytest.mark.asyncio
async def test_mock_context_wires_all_surfaces():
    """The factory-built MockContext exposes every ctx.* surface."""
    # GIVEN a context built by the testing factory
    # WHEN each documented surface is read
    ctx = MockContext()

    # THEN none of them is None, so an agent can exercise every surface
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
    # GIVEN two contexts built by the same factory
    ctx1 = MockContext()
    ctx2 = MockContext()

    # WHEN their surfaces are compared by identity
    # THEN no surface is shared, so one test cannot pollute another
    assert ctx1.tools is not ctx2.tools
    assert ctx1.llm is not ctx2.llm
    assert ctx1.events is not ctx2.events


@pytest.mark.asyncio
async def test_mock_tool_proxy_records_calls():
    """MockToolProxy records each call and returns the configured response."""
    # GIVEN a tool proxy configured with one canned response for "bash"
    proxy = MockToolProxy({"bash": {"output": "ok", "exit_code": 0}})

    # WHEN the tool is called
    result = await proxy.call("bash", {"cmd": "ls"})

    # THEN the canned response comes back and the call is recorded verbatim
    assert result == {"output": "ok", "exit_code": 0}
    assert proxy.calls == [("bash", {"cmd": "ls"})]
    assert proxy.tool_call_count() == 1


@pytest.mark.asyncio
async def test_mock_tool_proxy_assert_called_with():
    """assert_called_with passes for matching calls and fails otherwise."""
    # GIVEN a proxy that recorded one call to "bash" with a known payload
    proxy = MockToolProxy({"bash": {"output": "ok"}})
    await proxy.call("bash", {"cmd": "ls"})

    # WHEN the recorded call is asserted, then a call that never happened
    proxy.assert_called("bash")
    proxy.assert_called_with("bash", {"cmd": "ls"})

    # THEN the first pair passes and both mismatches raise
    with pytest.raises(AssertionError):
        proxy.assert_called("python")

    with pytest.raises(AssertionError):
        proxy.assert_called_with("bash", {"cmd": "pwd"})


@pytest.mark.asyncio
async def test_mock_tool_proxy_unknown_tool_raises():
    """Calling an unconfigured tool raises KeyError."""
    # GIVEN a proxy configured for "bash" only
    proxy = MockToolProxy({"bash": {"output": "ok"}})

    # WHEN an unconfigured tool is called
    # THEN it raises instead of returning an empty result
    with pytest.raises(KeyError, match="python"):
        await proxy.call("python", {})


@pytest.mark.asyncio
async def test_mock_tool_proxy_list_and_describe():
    """list_tools and describe expose configured tool metadata."""
    # GIVEN a proxy configured with two tools
    proxy = MockToolProxy({"bash": {"output": "ok"}, "python": {"output": "hi"}})

    # WHEN the catalogue is listed and one tool is described
    assert sorted(proxy.list_tools()) == ["bash", "python"]

    desc = await proxy.describe("bash")

    # THEN the configured tools are described and an unknown one renders None
    assert desc is not None
    assert desc["name"] == "bash"

    assert await proxy.describe("unknown") is None


@pytest.mark.asyncio
async def test_mock_llm_proxy_sequential_responses():
    """MockLlmProxy returns responses in FIFO order then raises on exhaustion."""
    # GIVEN a proxy with two queued responses
    proxy = MockLlmProxy([{"content": "first"}, {"content": "second"}])

    # WHEN it is completed twice, then a third time
    r1 = await proxy.complete("prompt1")
    r2 = await proxy.complete("prompt2")

    # THEN the queue is consumed in order and the third call runs dry
    # complete() returns a MockLlmResponse, accessed via .content or [].
    assert r1.content == "first"
    assert r2.content == "second"
    assert r1["content"] == "first"
    proxy.assert_called_count(2)

    with pytest.raises(IndexError):
        await proxy.complete("prompt3")


@pytest.mark.asyncio
async def test_mock_llm_proxy_map_preserves_order_and_records():
    """MockLlmProxy.map returns one ok result per item, in order, and records."""
    # GIVEN a proxy with no queued response and three items to map over
    proxy = MockLlmProxy()
    items = ["alpha", "beta", "gamma"]

    # WHEN the items are mapped
    results = await proxy.map(prefix="Classify:", items=items)

    # THEN one result per item comes back in order, and the call is recorded
    assert [r["index"] for r in results] == [0, 1, 2]
    assert all(r["ok"] for r in results)
    # With no queued responses the mock echoes each item back.
    assert [r["text"] for r in results] == items
    assert proxy.map_calls == [{"prefix": "Classify:", "items": items}]


@pytest.mark.asyncio
async def test_mock_llm_proxy_chat():
    """MockLlmProxy.chat() delegates to complete() and records the call."""
    # GIVEN a proxy with one queued response
    proxy = MockLlmProxy([{"content": "reply"}])

    # WHEN chat() is used rather than complete()
    result = await proxy.chat(system="You are helpful.", user="Hello")

    # THEN the queued answer comes back and the call lands in the same records
    assert result.content == "reply"
    assert proxy.call_count == 1
    assert len(proxy.prompts) == 1
    assert proxy.default_backend == "mock"


@pytest.mark.asyncio
async def test_mock_llm_proxy_chat_accepts_sampling_overrides():
    """MockLlmProxy.chat() accepts temperature/max_tokens/seed like the real proxy."""
    # GIVEN a proxy with one queued response
    proxy = MockLlmProxy([{"content": "ok"}])

    # WHEN chat() is called with the sampling overrides of the real proxy
    result = await proxy.chat(
        system="sys",
        user="usr",
        temperature=0.2,
        max_tokens=64,
        seed=7,
    )

    # THEN the signature is accepted, so a test written against the mock ports over
    assert result.content == "ok"
    assert proxy.call_count == 1


@pytest.mark.asyncio
async def test_mock_llm_proxy_assert_called_count_failure():
    """assert_called_count raises when actual differs from expected."""
    # GIVEN a proxy that was completed once
    proxy = MockLlmProxy([{"content": "a"}])
    await proxy.complete("p")

    # WHEN five calls are asserted
    # THEN it raises and the message names the expected count
    with pytest.raises(AssertionError, match="Expected 5"):
        proxy.assert_called_count(5)


@pytest.mark.asyncio
async def test_mock_memory_record_and_recall():
    """MockMemory stores episodic events and semantic key/values."""
    # GIVEN an empty mock memory
    mem = MockMemory()

    # WHEN an episode is recorded, a key remembered, then recalled
    await mem.record("user logged in", importance=0.8)
    await mem.remember("user_name", "alice")
    result = await mem.recall("user_name")

    # THEN the value comes back and all three operations are journalled
    assert result == "alice"
    assert len(mem.episodes) == 1
    assert mem.episodes[0]["content"] == "user logged in"
    assert len(mem.operations) == 3


@pytest.mark.asyncio
async def test_mock_memory_recall_missing_key():
    """Recalling a non-existent key returns None."""
    # GIVEN an empty mock memory
    mem = MockMemory()

    # WHEN an unknown key is recalled
    result = await mem.recall("missing")

    # THEN None comes back instead of a KeyError
    assert result is None


@pytest.mark.asyncio
async def test_mock_memory_forget():
    """forget() removes the entry and records the operation."""
    # GIVEN a mock memory holding one key
    mem = MockMemory()
    await mem.remember("key1", "value1")

    # WHEN the key is forgotten
    await mem.forget("key1")

    # THEN it can no longer be recalled and the journal says it existed
    assert await mem.recall("key1") is None
    forget_op = next(op for op in mem.operations if op["op"] == "forget")
    assert forget_op["existed"] is True


@pytest.mark.asyncio
async def test_mock_memory_forget_nonexistent():
    """forget() on a missing key records existed=False."""
    # GIVEN an empty mock memory
    mem = MockMemory()

    # WHEN an absent key is forgotten
    await mem.forget("nope")

    # THEN the journal records the miss rather than dropping the operation
    forget_op = next(op for op in mem.operations if op["op"] == "forget")
    assert forget_op["existed"] is False


@pytest.mark.asyncio
async def test_mock_memory_search():
    """search() returns entries whose key contains the query string."""
    # GIVEN three remembered keys, two of which share a prefix
    mem = MockMemory()
    await mem.remember("user_name", "alice")
    await mem.remember("user_email", "alice@example.com")
    await mem.remember("system_version", "1.0")

    # WHEN the shared prefix is searched
    results = await mem.search("user")

    # THEN only the two matching entries come back, tagged as mock results
    assert len(results) == 2
    assert all(r["source"] == "mock" for r in results)
