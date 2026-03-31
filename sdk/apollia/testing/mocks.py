"""Mock implementations of Apollia runtime objects for agent unit testing.

Provides drop-in replacements for ``ToolProxy``, ``LlmProxy``,
``MemoryInterface``, and ``RuntimeContext`` so agents can be tested in
isolation without a running Apollia runtime.

Example::

    from apollia.testing import MockContext

    ctx = MockContext.create(
        tools={"bash": {"output": "hello"}},
        llm_responses=[{"content": "answer"}],
        memory=True,
    )
    result = await my_agent.run(task, ctx)
"""

from __future__ import annotations

from typing import Any


class MockToolProxy:
    """In-memory mock of ``ToolProxy`` for agent unit tests.

    Attributes:
        calls: Ordered list of ``(tool_name, args)`` tuples recorded
            for every ``call()`` invocation.
        responses: Mapping of tool name to the dict returned by ``call()``.
    """

    def __init__(self, responses: dict[str, Any] | None = None) -> None:
        self.responses: dict[str, Any] = responses or {}
        self.calls: list[tuple[str, dict[str, object]]] = []

    async def call(
        self,
        tool_name: str,
        input: dict[str, object] | None = None,
    ) -> dict[str, object]:
        """Simulate a tool invocation.

        Records the call in ``self.calls`` and returns the pre-configured
        response.

        Raises:
            KeyError: If no response is configured for *tool_name*.
        """
        args: dict[str, object] = input or {}
        self.calls.append((tool_name, args))
        if tool_name not in self.responses:
            raise KeyError(f"No mock response configured for tool '{tool_name}'")
        result: dict[str, object] = self.responses[tool_name]
        return result

    def list_tools(self) -> list[str]:
        """Return the names of all configured mock tools."""
        return list(self.responses.keys())

    def tool_call_count(self) -> int:
        """Return the total number of tool calls recorded so far."""
        return len(self.calls)

    async def describe(self, name: str) -> dict[str, object] | None:
        """Return a minimal tool descriptor, or ``None`` if not configured."""
        if name not in self.responses:
            return None
        return {
            "name": name,
            "version": "mock",
            "description": f"Mock tool {name}",
            "input_schema": {},
            "output_schema": {},
            "tags": [],
        }

    def assert_called(self, name: str) -> None:
        """Assert that *name* was called at least once.

        Raises:
            AssertionError: If the tool was never called.
        """
        called_names = [n for n, _ in self.calls]
        if name not in called_names:
            raise AssertionError(
                f"Tool '{name}' was never called. "
                f"Calls recorded: {called_names}"
            )

    def assert_called_with(self, name: str, args: dict[str, object]) -> None:
        """Assert that *name* was called with exactly *args*.

        Raises:
            AssertionError: If no matching ``(name, args)`` pair is found.
        """
        if (name, args) not in self.calls:
            matching = [(n, a) for n, a in self.calls if n == name]
            raise AssertionError(
                f"Tool '{name}' was not called with {args}. "
                f"Matching calls: {matching}"
            )


class MockLlmResponse:
    """Attribute-accessible wrapper for LLM response dicts.

    The real ``LlmProxy`` (PyO3) returns an object with a ``.content``
    attribute.  This wrapper bridges the gap so that both
    ``response.content`` and ``response["text"]`` patterns work in tests.
    """

    def __init__(self, data: dict[str, object]) -> None:
        self._data = data
        text = data.get("text") or data.get("content") or ""
        self.content: str = str(text)
        self.text: str = self.content

    def get(self, key: str, default: object = None) -> object:
        return self._data.get(key, default)

    def __getitem__(self, key: str) -> object:
        return self._data[key]

    def __contains__(self, key: str) -> bool:
        return key in self._data


class MockLlmProxy:
    """In-memory mock of ``LlmProxy`` for agent unit tests.

    Responses are consumed in FIFO order. Each ``complete()`` or ``chat()``
    call pops the next response from the queue.

    Attributes:
        responses: Remaining response dicts to be consumed.
        call_count: Total number of completion calls made.
        prompts: Ordered list of prompts or message lists received.
    """

    def __init__(self, responses: list[dict[str, object]] | None = None) -> None:
        self.responses: list[dict[str, object]] = list(responses or [])
        self.call_count: int = 0
        self.prompts: list[Any] = []

    async def complete(
        self,
        messages: list[dict[str, object]] | str,
        **kwargs: Any,
    ) -> MockLlmResponse:
        """Consume and return the next queued response.

        Returns a ``MockLlmResponse`` with both ``.content`` attribute
        access (matching the real PyO3 ``LlmResponse``) and dict-style
        access for backward compatibility.

        Raises:
            IndexError: If no more responses are available.
        """
        self.prompts.append(messages)
        self.call_count += 1
        if not self.responses:
            raise IndexError(
                f"MockLlmProxy exhausted after {self.call_count} calls — "
                "no more responses configured"
            )
        return MockLlmResponse(self.responses.pop(0))

    async def chat(
        self,
        system: str,
        user: str,
        backend: str | None = None,
    ) -> dict[str, object]:
        """Convenience wrapper matching ``LlmProxy.chat()`` signature.

        Delegates to ``complete()`` internally.
        """
        return await self.complete(
            [{"role": "system", "content": system}, {"role": "user", "content": user}],
            backend=backend,
        )

    @property
    def default_backend(self) -> str:
        """Return a fixed mock backend name."""
        return "mock"

    def assert_called_count(self, expected: int) -> None:
        """Assert that exactly *expected* calls were made.

        Raises:
            AssertionError: If the actual count differs.
        """
        if self.call_count != expected:
            raise AssertionError(
                f"Expected {expected} LLM calls, got {self.call_count}"
            )


class MockMemory:
    """In-memory mock of ``MemoryInterface`` for agent unit tests.

    Uses a flat key/value dict for semantic memory and records every
    operation in ``self.operations``.

    Attributes:
        store: In-memory semantic key/value storage.
        confidences: Per-key confidence scores (0.0–1.0).
        episodes: In-memory episodic events.
        operations: Ordered list of ``{"op": ..., ...}`` dicts for
            introspection.
    """

    def __init__(self) -> None:
        self.store: dict[str, str] = {}
        self.confidences: dict[str, float] = {}
        self.episodes: list[dict[str, Any]] = []
        self.operations: list[dict[str, Any]] = []

    async def record(
        self,
        content: str,
        importance: float | None = None,
        task_id: str | None = None,
    ) -> None:
        """Record an episodic memory event."""
        entry = {
            "content": content,
            "importance": importance if importance is not None else 0.5,
            "task_id": task_id,
        }
        self.episodes.append(entry)
        self.operations.append({"op": "record", **entry})

    async def remember(
        self,
        key: str,
        value: str,
        source: str | None = None,
        confidence: float | None = None,
    ) -> None:
        """Store a key/value pair in semantic memory.

        When *confidence* is provided and the key already exists with
        a strictly higher confidence, the write is skipped.
        """
        effective_confidence = confidence if confidence is not None else 1.0

        if confidence is not None and key in self.confidences:
            if self.confidences[key] > effective_confidence:
                self.operations.append({
                    "op": "remember",
                    "key": key,
                    "value": value,
                    "source": source,
                    "confidence": effective_confidence,
                    "skipped": True,
                })
                return

        self.store[key] = value
        self.confidences[key] = effective_confidence
        self.operations.append({
            "op": "remember",
            "key": key,
            "value": value,
            "source": source,
            "confidence": effective_confidence,
            "skipped": False,
        })

    async def recall(self, key: str) -> str | None:
        """Retrieve a value by key from semantic memory."""
        result = self.store.get(key)
        self.operations.append({"op": "recall", "key": key, "found": result is not None})
        return result

    async def search(
        self,
        query: str,
        limit: int | None = None,
    ) -> list[dict[str, object]]:
        """Full-text search simulation.

        Returns entries whose key contains *query*, capped at *limit*.
        """
        effective_limit = limit if limit is not None else 10
        results: list[dict[str, object]] = [
            {"content": v, "score": 1.0, "source": "mock", "timestamp": ""}
            for k, v in self.store.items()
            if query.lower() in k.lower()
        ][:effective_limit]
        self.operations.append({"op": "search", "query": query, "results": len(results)})
        return results

    async def forget(self, key: str) -> None:
        """Remove a key/value pair from semantic memory."""
        existed = key in self.store
        self.store.pop(key, None)
        self.operations.append({"op": "forget", "key": key, "existed": existed})


class MockContext:
    """Factory assembling a complete mock execution context.

    Instantiate via the ``create()`` class method — do not use the
    constructor directly.

    Example::

        ctx = MockContext.create(
            tools={"bash": {"output": "hello"}},
            llm_responses=[{"content": "answer"}],
            memory=True,
        )
        assert ctx.tools is not None
        result = await ctx.tools.call("bash", {"cmd": "echo hi"})
    """

    def __init__(
        self,
        *,
        _tools: MockToolProxy | None = None,
        _llm: MockLlmProxy | None = None,
        _memory: MockMemory | None = None,
    ) -> None:
        self._tools = _tools
        self._llm = _llm
        self._memory = _memory

    @staticmethod
    def create(
        tools: dict[str, Any] | None = None,
        llm_responses: list[dict[str, object]] | None = None,
        memory: bool = False,
    ) -> MockContext:
        """Build a ``MockContext`` with the requested components.

        Args:
            tools: Mapping of tool name to response dict.  ``None`` means
                no ``ToolProxy`` is attached.
            llm_responses: Ordered list of response dicts for the LLM.
                ``None`` means no ``LlmProxy`` is attached.
            memory: If ``True``, attach an empty ``MockMemory``.

        Returns:
            A fully wired ``MockContext`` ready for injection into an agent.
        """
        return MockContext(
            _tools=MockToolProxy(tools) if tools is not None else None,
            _llm=MockLlmProxy(llm_responses) if llm_responses is not None else None,
            _memory=MockMemory() if memory else None,
        )

    @property
    def tools(self) -> MockToolProxy | None:
        """Tool proxy mock, or ``None`` if not configured."""
        return self._tools

    @property
    def llm(self) -> MockLlmProxy | None:
        """LLM proxy mock, or ``None`` if not configured."""
        return self._llm

    @property
    def memory(self) -> MockMemory | None:
        """Memory interface mock, or ``None`` if not configured."""
        return self._memory
