from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="WorkflowStepView")


@_attrs_define
class WorkflowStepView:
    """One declared step of a run.

    Attributes:
        name (str):
        rank (int): Its position among the declared steps, from 0.
        status (str): `pending`, `running`, `completed`, `failed`, `canceled` or
            `not_started`.
        ended_at (None | str | Unset):
        error (None | str | Unset):
        output (None | str | Unset): What the agent or workflow the step ran last returned.
        started_at (None | str | Unset):
    """

    name: str
    rank: int
    status: str
    ended_at: None | str | Unset = UNSET
    error: None | str | Unset = UNSET
    output: None | str | Unset = UNSET
    started_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        name = self.name

        rank = self.rank

        status = self.status

        ended_at: None | str | Unset
        if isinstance(self.ended_at, Unset):
            ended_at = UNSET
        else:
            ended_at = self.ended_at

        error: None | str | Unset
        if isinstance(self.error, Unset):
            error = UNSET
        else:
            error = self.error

        output: None | str | Unset
        if isinstance(self.output, Unset):
            output = UNSET
        else:
            output = self.output

        started_at: None | str | Unset
        if isinstance(self.started_at, Unset):
            started_at = UNSET
        else:
            started_at = self.started_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "name": name,
                "rank": rank,
                "status": status,
            }
        )
        if ended_at is not UNSET:
            field_dict["endedAt"] = ended_at
        if error is not UNSET:
            field_dict["error"] = error
        if output is not UNSET:
            field_dict["output"] = output
        if started_at is not UNSET:
            field_dict["startedAt"] = started_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        name = d.pop("name")

        rank = d.pop("rank")

        status = d.pop("status")

        def _parse_ended_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        ended_at = _parse_ended_at(d.pop("endedAt", UNSET))

        def _parse_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error = _parse_error(d.pop("error", UNSET))

        def _parse_output(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        output = _parse_output(d.pop("output", UNSET))

        def _parse_started_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        started_at = _parse_started_at(d.pop("startedAt", UNSET))

        workflow_step_view = cls(
            name=name,
            rank=rank,
            status=status,
            ended_at=ended_at,
            error=error,
            output=output,
            started_at=started_at,
        )

        workflow_step_view.additional_properties = d
        return workflow_step_view

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
