from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="AuditStatsResponse")



@_attrs_define
class AuditStatsResponse:
    """ Response body for `GET /api/v1/audit/stats`.

        Attributes:
            total_events (int):
            unique_agents (int):
            unique_tools (int):
     """

    total_events: int
    unique_agents: int
    unique_tools: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        total_events = self.total_events

        unique_agents = self.unique_agents

        unique_tools = self.unique_tools


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "total_events": total_events,
            "unique_agents": unique_agents,
            "unique_tools": unique_tools,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        total_events = d.pop("total_events")

        unique_agents = d.pop("unique_agents")

        unique_tools = d.pop("unique_tools")

        audit_stats_response = cls(
            total_events=total_events,
            unique_agents=unique_agents,
            unique_tools=unique_tools,
        )


        audit_stats_response.additional_properties = d
        return audit_stats_response

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
