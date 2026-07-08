"""ctx.llm - LLM completion, streaming, embeddings (Protocol)."""

from __future__ import annotations

from collections.abc import AsyncIterator
from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

if TYPE_CHECKING:
    # Runtime import would be circular (apollia.types imports LlmProxy from here);
    # only needed for annotations, which are lazy under `from __future__`.
    from apollia.types import MapItemResult


@runtime_checkable
class TokenUsage(Protocol):
    """Token usage stats for an LLM call."""

    prompt_tokens: int
    completion_tokens: int
    cost_usd: float


@runtime_checkable
class LlmResponse(Protocol):
    """Synchronous LLM response (after completion)."""

    content: str
    latency_ms: int

    @property
    def usage(self) -> TokenUsage: ...


@runtime_checkable
class LlmProxy(Protocol):
    """``ctx.llm`` - LLM backend access.

    Three primary methods: :meth:`complete` for single-shot, :meth:`stream`
    for token iteration, :meth:`embed` for embeddings.  Stream cleanup
    propagates cancellation to the Rust backend.
    """

    @property
    def default_backend(self) -> str: ...

    async def complete(
        self,
        messages: list[dict[str, Any]],
        *,
        backend: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        seed: int | None = None,
    ) -> LlmResponse: ...

    async def chat(
        self,
        system: str,
        user: str,
        *,
        backend: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        seed: int | None = None,
    ) -> LlmResponse: ...

    async def map(
        self,
        prefix: str,
        items: list[str],
        *,
        backend: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        max_concurrency: int | None = None,
    ) -> list[MapItemResult]:
        """Batch a shared-prefix prompt over many items.

        Each item is completed as ``[system(prefix), user(item)]``, so the
        system message is identical across items. Apollia owns the concurrency
        and prefix sharing: on a batching backend (a local ``llama-server``) this
        maximizes prefix-cache reuse and continuous batching; elsewhere it
        degrades to bounded-concurrent calls. You never touch slots or batching.

        Results are order-preserving, one :class:`~apollia.types.MapItemResult`
        per item. A single failing item never aborts the batch. Typical usage::

            results = await ctx.llm.map(prefix=instructions, items=paragraphs)
            for r in results:
                if r["ok"]:
                    use(r["text"])
        """
        ...

    async def stream(
        self,
        messages: list[dict[str, Any]],
        *,
        backend: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        seed: int | None = None,
    ) -> AsyncIterator[str]:
        """Open a streaming completion.

        The method itself is ``async`` (it must be ``await``-ed) and
        returns an :class:`~collections.abc.AsyncIterator` over token
        deltas. Typical usage::

            stream = await ctx.llm.stream(messages=[...])
            async for token in stream:
                ctx.events.emit_token(token)

        Cancellation propagates to the Rust backend on iterator close.
        """
        ...

    async def embed(
        self,
        text: str,
        *,
        backend: str | None = None,
    ) -> list[float]: ...
