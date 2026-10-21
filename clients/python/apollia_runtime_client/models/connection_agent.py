from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ConnectionAgent")


@_attrs_define
class ConnectionAgent:
    """One agent that holds at least one tool of a connection.

    Attributes:
        by_default (bool): Whether the agent holds these tools through the default access, nobody
            having chosen its access.
        enabled (bool): Whether the agent is active.
        id (str): Identifier of the agent.
        name (str): Display name of the agent.
        tools (list[str]): The tools of the connection the agent holds, in the order of its
            access.
    """

    by_default: bool
    enabled: bool
    id: str
    name: str
    tools: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        by_default = self.by_default

        enabled = self.enabled

        id = self.id

        name = self.name

        tools = self.tools

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "byDefault": by_default,
                "enabled": enabled,
                "id": id,
                "name": name,
                "tools": tools,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        by_default = d.pop("byDefault")

        enabled = d.pop("enabled")

        id = d.pop("id")

        name = d.pop("name")

        tools = cast(list[str], d.pop("tools"))

        connection_agent = cls(
            by_default=by_default,
            enabled=enabled,
            id=id,
            name=name,
            tools=tools,
        )

        connection_agent.additional_properties = d
        return connection_agent

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
