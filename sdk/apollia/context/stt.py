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
    ) -> str:
        """Transcribe an audio file to text.

        Args:
            path: Path to the audio file, resolved inside the workspace.
            language: BCP 47 hint for the spoken language, or None to let the
                backend detect it.
            backend: Backend to use, or None for the configured default.

        Returns:
            The transcript.
        """
        ...

    def status(self) -> dict[str, Any]:
        """Return backend readiness: loaded model, device and sample rate."""
        ...
