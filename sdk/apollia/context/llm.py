"""ctx.llm — LLM completion, streaming, embeddings (Protocol)."""

from __future__ import annotations

from collections.abc import AsyncIterator
from typing import Any, Protocol, runtime_checkable


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
    """``ctx.llm`` — LLM backend access.

    Three primary methods: :meth:`complete` for single-shot, :meth:`stream`
    for token iteration, :meth:`embed` for embeddings.  Stream cleanup is
    governed by ADR-112 (cancellation propagates to the Rust backend).
    """

    @property
    def default_backend(self) -> str: ...

    async def complete(
        self,
        messages: list[dict[str, Any]],
        *,
        backend: str | None = None,
        temperature: float = 0.7,
        max_tokens: int | None = None,
    ) -> LlmResponse: ...

    async def chat(
        self,
        system: str,
        user: str,
        *,
        backend: str | None = None,
        temperature: float = 0.7,
    ) -> LlmResponse: ...

    def stream(
        self,
        messages: list[dict[str, Any]],
        *,
        backend: str | None = None,
        temperature: float = 0.7,
    ) -> AsyncIterator[str]: ...

    async def embed(
        self,
        text: str,
        *,
        backend: str | None = None,
    ) -> list[float]: ...
