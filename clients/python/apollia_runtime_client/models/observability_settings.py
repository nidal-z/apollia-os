from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ObservabilitySettings")


@_attrs_define
class ObservabilitySettings:
    """The observability policy, one form over the two sections that hold it.

    Every field but `debug_log_prompt` is a key of `[observability]`;
    `debug_log_prompt` is the key of `[llm.observability]`, read by a different
    type, and is written there: under `[observability]` nothing would read it.
    An absent field reads as its default.

        Attributes:
            capture_agent_logs (bool | Unset): Whether the logs an agent writes are kept on its trace. Default: True.
            capture_thoughts (bool | Unset): Whether the reasoning of an agent is kept on its trace. Default: True.
            capture_tool_args (bool | Unset): Whether the arguments of tool calls are kept. Default: True.
            capture_tool_outputs (bool | Unset): Whether the outputs of tool calls are kept. Default: True.
            debug_log_prompt (bool | Unset): Logs the whole prompt at `TRACE`, and persists nothing. Default: False.
            max_input_bytes (int | Unset): Max size of task and step inputs, in bytes. Default: 32768.
            max_output_bytes (int | Unset): Max size of task, step and completion outputs, in bytes. Default: 32768.
            max_tool_output_bytes (int | Unset): Max size of a tool's stdout and stderr, in bytes. Default: 10240.
            retention_days (int | Unset): Days the trace is kept before it is purged. Default: 90.
    """

    capture_agent_logs: bool | Unset = True
    capture_thoughts: bool | Unset = True
    capture_tool_args: bool | Unset = True
    capture_tool_outputs: bool | Unset = True
    debug_log_prompt: bool | Unset = False
    max_input_bytes: int | Unset = 32768
    max_output_bytes: int | Unset = 32768
    max_tool_output_bytes: int | Unset = 10240
    retention_days: int | Unset = 90
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        capture_agent_logs = self.capture_agent_logs

        capture_thoughts = self.capture_thoughts

        capture_tool_args = self.capture_tool_args

        capture_tool_outputs = self.capture_tool_outputs

        debug_log_prompt = self.debug_log_prompt

        max_input_bytes = self.max_input_bytes

        max_output_bytes = self.max_output_bytes

        max_tool_output_bytes = self.max_tool_output_bytes

        retention_days = self.retention_days

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if capture_agent_logs is not UNSET:
            field_dict["capture_agent_logs"] = capture_agent_logs
        if capture_thoughts is not UNSET:
            field_dict["capture_thoughts"] = capture_thoughts
        if capture_tool_args is not UNSET:
            field_dict["capture_tool_args"] = capture_tool_args
        if capture_tool_outputs is not UNSET:
            field_dict["capture_tool_outputs"] = capture_tool_outputs
        if debug_log_prompt is not UNSET:
            field_dict["debug_log_prompt"] = debug_log_prompt
        if max_input_bytes is not UNSET:
            field_dict["max_input_bytes"] = max_input_bytes
        if max_output_bytes is not UNSET:
            field_dict["max_output_bytes"] = max_output_bytes
        if max_tool_output_bytes is not UNSET:
            field_dict["max_tool_output_bytes"] = max_tool_output_bytes
        if retention_days is not UNSET:
            field_dict["retention_days"] = retention_days

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        capture_agent_logs = d.pop("capture_agent_logs", UNSET)

        capture_thoughts = d.pop("capture_thoughts", UNSET)

        capture_tool_args = d.pop("capture_tool_args", UNSET)

        capture_tool_outputs = d.pop("capture_tool_outputs", UNSET)

        debug_log_prompt = d.pop("debug_log_prompt", UNSET)

        max_input_bytes = d.pop("max_input_bytes", UNSET)

        max_output_bytes = d.pop("max_output_bytes", UNSET)

        max_tool_output_bytes = d.pop("max_tool_output_bytes", UNSET)

        retention_days = d.pop("retention_days", UNSET)

        observability_settings = cls(
            capture_agent_logs=capture_agent_logs,
            capture_thoughts=capture_thoughts,
            capture_tool_args=capture_tool_args,
            capture_tool_outputs=capture_tool_outputs,
            debug_log_prompt=debug_log_prompt,
            max_input_bytes=max_input_bytes,
            max_output_bytes=max_output_bytes,
            max_tool_output_bytes=max_tool_output_bytes,
            retention_days=retention_days,
        )

        observability_settings.additional_properties = d
        return observability_settings

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
