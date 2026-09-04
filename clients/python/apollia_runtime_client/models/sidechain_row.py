from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="SidechainRow")



@_attrs_define
class SidechainRow:
    """ Row returned by [`SidechainRepository::list_by_parent`].

        Attributes:
            agent_name (str): Target agent name (or the skill_id used for resolution).
            sidechain_n (int): Sequential delegation number for this parent (1-based).
            status (str): Current status: `"running"`, `"completed"`, or `"failed"`.
            completed_at (None | str | Unset): ISO 8601 timestamp when the delegation finished (`None` if still running).
            input_summary (None | str | Unset): First 500 characters of the input.
            output_summary (None | str | Unset): First 500 characters of the output or the error message.
            started_at (None | str | Unset): ISO 8601 timestamp when the delegation started.
     """

    agent_name: str
    sidechain_n: int
    status: str
    completed_at: None | str | Unset = UNSET
    input_summary: None | str | Unset = UNSET
    output_summary: None | str | Unset = UNSET
    started_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        agent_name = self.agent_name

        sidechain_n = self.sidechain_n

        status = self.status

        completed_at: None | str | Unset
        if isinstance(self.completed_at, Unset):
            completed_at = UNSET
        else:
            completed_at = self.completed_at

        input_summary: None | str | Unset
        if isinstance(self.input_summary, Unset):
            input_summary = UNSET
        else:
            input_summary = self.input_summary

        output_summary: None | str | Unset
        if isinstance(self.output_summary, Unset):
            output_summary = UNSET
        else:
            output_summary = self.output_summary

        started_at: None | str | Unset
        if isinstance(self.started_at, Unset):
            started_at = UNSET
        else:
            started_at = self.started_at


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agent_name": agent_name,
            "sidechain_n": sidechain_n,
            "status": status,
        })
        if completed_at is not UNSET:
            field_dict["completed_at"] = completed_at
        if input_summary is not UNSET:
            field_dict["input_summary"] = input_summary
        if output_summary is not UNSET:
            field_dict["output_summary"] = output_summary
        if started_at is not UNSET:
            field_dict["started_at"] = started_at

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent_name = d.pop("agent_name")

        sidechain_n = d.pop("sidechain_n")

        status = d.pop("status")

        def _parse_completed_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        completed_at = _parse_completed_at(d.pop("completed_at", UNSET))


        def _parse_input_summary(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        input_summary = _parse_input_summary(d.pop("input_summary", UNSET))


        def _parse_output_summary(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        output_summary = _parse_output_summary(d.pop("output_summary", UNSET))


        def _parse_started_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        started_at = _parse_started_at(d.pop("started_at", UNSET))


        sidechain_row = cls(
            agent_name=agent_name,
            sidechain_n=sidechain_n,
            status=status,
            completed_at=completed_at,
            input_summary=input_summary,
            output_summary=output_summary,
            started_at=started_at,
        )


        sidechain_row.additional_properties = d
        return sidechain_row

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
