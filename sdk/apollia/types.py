"""Core types shared across the Apollia SDK."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class AIPResult:
    """Result returned by an agent's run() method to the Apollia runtime.

    The runtime deserializes this via ``to_dict()`` to determine the outcome
    of a task execution.  Three factory methods cover the common cases:

    * ``AIPResult.completed(text)`` — successful execution
    * ``AIPResult.failed(code, message)`` — error with structured info
    * ``AIPResult.input_required(prompt)`` — HITL pause awaiting user input
    """

    status: str
    text: str | None = None
    error_code: str | None = None
    error_message: str | None = None
    input_prompt: str | None = None
    input_context: dict[str, Any] | None = None
    data: dict[str, Any] = field(default_factory=dict)

    @staticmethod
    def completed(text: str, data: dict[str, Any] | None = None) -> AIPResult:
        """Create a successful result."""
        return AIPResult(status="completed", text=text, data=data or {})

    @staticmethod
    def failed(code: str, message: str) -> AIPResult:
        """Create a failure result."""
        return AIPResult(status="failed", error_code=code, error_message=message)

    @staticmethod
    def input_required(
        prompt: str, context: dict[str, Any] | None = None
    ) -> AIPResult:
        """Create an input-required result (HITL)."""
        return AIPResult(
            status="input_required", input_prompt=prompt, input_context=context
        )

    def to_dict(self) -> dict[str, Any]:
        """Serialize to dict for runtime consumption.

        Only fields with non-``None`` values (and non-empty ``data``) are
        included so the runtime receives a compact payload.
        """
        result: dict[str, Any] = {"status": self.status}
        if self.text is not None:
            result["text"] = self.text
        if self.error_code is not None:
            result["error_code"] = self.error_code
        if self.error_message is not None:
            result["error_message"] = self.error_message
        if self.input_prompt is not None:
            result["input_prompt"] = self.input_prompt
        if self.input_context is not None:
            result["input_context"] = self.input_context
        if self.data:
            result["data"] = self.data
        return result
