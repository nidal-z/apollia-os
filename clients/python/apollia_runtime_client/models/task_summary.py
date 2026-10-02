from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="TaskSummary")


@_attrs_define
class TaskSummary:
    """One task of the view.

    Attributes:
        agent_id (str): Runtime identifier of the agent, empty for a task read from the store.
        agent_name (str): Name of the agent.
        created_at (str): ISO 8601 creation date, empty when the store has no row yet.
        id (str): Unique task identifier.
        input_preview (str): The first characters of the input.
        status (str): Current status.
        duration_ms (int | None | Unset): Execution duration in milliseconds.
        output_text (None | str | Unset): Output text, as the store kept it.
    """

    agent_id: str
    agent_name: str
    created_at: str
    id: str
    input_preview: str
    status: str
    duration_ms: int | None | Unset = UNSET
    output_text: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent_id = self.agent_id

        agent_name = self.agent_name

        created_at = self.created_at

        id = self.id

        input_preview = self.input_preview

        status = self.status

        duration_ms: int | None | Unset
        if isinstance(self.duration_ms, Unset):
            duration_ms = UNSET
        else:
            duration_ms = self.duration_ms

        output_text: None | str | Unset
        if isinstance(self.output_text, Unset):
            output_text = UNSET
        else:
            output_text = self.output_text

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent_id": agent_id,
                "agent_name": agent_name,
                "created_at": created_at,
                "id": id,
                "input_preview": input_preview,
                "status": status,
            }
        )
        if duration_ms is not UNSET:
            field_dict["duration_ms"] = duration_ms
        if output_text is not UNSET:
            field_dict["output_text"] = output_text

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent_id = d.pop("agent_id")

        agent_name = d.pop("agent_name")

        created_at = d.pop("created_at")

        id = d.pop("id")

        input_preview = d.pop("input_preview")

        status = d.pop("status")

        def _parse_duration_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        duration_ms = _parse_duration_ms(d.pop("duration_ms", UNSET))

        def _parse_output_text(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        output_text = _parse_output_text(d.pop("output_text", UNSET))

        task_summary = cls(
            agent_id=agent_id,
            agent_name=agent_name,
            created_at=created_at,
            id=id,
            input_preview=input_preview,
            status=status,
            duration_ms=duration_ms,
            output_text=output_text,
        )

        task_summary.additional_properties = d
        return task_summary

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
