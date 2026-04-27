"""Type stubs for SttInterface — Python proxy to STT engine.

This class mirrors the ``#[pyclass]`` / ``#[pymethods]`` definition in
``crates/apollia-aip/src/stt.rs``.  It exists purely to provide IDE
autocompletion and ``mypy`` validation — the real implementation lives in
Rust and is injected at runtime via PyO3.
"""

from __future__ import annotations

from typing import Awaitable


class SttInterface:
    """Agent speech-to-text proxy exposed via ``ctx.stt``.

    Transcribes audio files using the configured STT backend.
    Raises ``RuntimeError`` when no backend is available.
    """

    async def transcribe(
        self,
        path: str,
        *,
        language: str | None = None,
        backend: str | None = None,
    ) -> str:
        """Transcribe an audio file. Returns the full text transcript.

        ``path`` must be a WAV file (PCM 16-bit or float32, mono or stereo).
        ``language`` is an optional ISO 639-1 language hint (e.g. ``"fr"``, ``"en"``).
        Raises ``RuntimeError`` if no STT backend is configured.
        Raises ``RuntimeError`` if the file does not exist.
        """
        ...

    async def status(self) -> dict:
        """Return STT engine status.

        Returns ``{"enabled": bool, "model": str | None, "language": str}``.
        """
        ...
