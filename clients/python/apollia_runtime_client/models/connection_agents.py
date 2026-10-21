from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.connection_agent import ConnectionAgent


T = TypeVar("T", bound="ConnectionAgents")


@_attrs_define
class ConnectionAgents:
    """The agents that hold the tools of one connection.

    Attributes:
        agents (list[ConnectionAgent]): Every agent holding at least one of its tools, by identifier.
        connection (str): Identifier of the connection.
        shared_with (list[str]): Other connections its tools reach as well, by identifier: the other
            mailboxes, for a mailbox, since the mailbox tools are common to all of
            them. Empty for every other kind.
    """

    agents: list[ConnectionAgent]
    connection: str
    shared_with: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        connection = self.connection

        shared_with = self.shared_with

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "connection": connection,
                "sharedWith": shared_with,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.connection_agent import ConnectionAgent

        d = dict(src_dict)
        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = ConnectionAgent.from_dict(agents_item_data)

            agents.append(agents_item)

        connection = d.pop("connection")

        shared_with = cast(list[str], d.pop("sharedWith"))

        connection_agents = cls(
            agents=agents,
            connection=connection,
            shared_with=shared_with,
        )

        connection_agents.additional_properties = d
        return connection_agents

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
