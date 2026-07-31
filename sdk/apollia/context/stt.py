"""ctx.stt - Speech-to-Text."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable


@runtime_checkable
class SttInterface(Protocol):
    """Audio transcription surface backed by ``apollia-stt``."""

    async def transcribe(
        self,
        path: str,
        *,
        language: str | None = None,
        backend: str | None = None,
    ) -> str: ...

    def status(self) -> dict[str, Any]: ...
