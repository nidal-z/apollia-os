from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.agent_activity import AgentActivity


T = TypeVar("T", bound="ConnectionActivityResponse")


@_attrs_define
class ConnectionActivityResponse:
    """Response body of `GET /api/v1/audit/activity`.

    Attributes:
        actions (int): Tool calls of the connection that ended over the period.
        agents (list[AgentActivity]): One line per agent, the most active first.
        since (str): Start of the period read, RFC 3339 UTC.
        tool_prefix (str): The prefix the activity was read for.
        last_used_at (None | str | Unset): When the last of them ended, RFC 3339 UTC; `None` when none did.
        until (None | str | Unset): End of the period read, RFC 3339 UTC; `None` when it has no end.
    """

    actions: int
    agents: list[AgentActivity]
    since: str
    tool_prefix: str
    last_used_at: None | str | Unset = UNSET
    until: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        actions = self.actions

        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        since = self.since

        tool_prefix = self.tool_prefix

        last_used_at: None | str | Unset
        if isinstance(self.last_used_at, Unset):
            last_used_at = UNSET
        else:
            last_used_at = self.last_used_at

        until: None | str | Unset
        if isinstance(self.until, Unset):
            until = UNSET
        else:
            until = self.until

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "actions": actions,
                "agents": agents,
                "since": since,
                "toolPrefix": tool_prefix,
            }
        )
        if last_used_at is not UNSET:
            field_dict["lastUsedAt"] = last_used_at
        if until is not UNSET:
            field_dict["until"] = until

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_activity import AgentActivity

        d = dict(src_dict)
        actions = d.pop("actions")

        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = AgentActivity.from_dict(agents_item_data)

            agents.append(agents_item)

        since = d.pop("since")

        tool_prefix = d.pop("toolPrefix")

        def _parse_last_used_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_used_at = _parse_last_used_at(d.pop("lastUsedAt", UNSET))

        def _parse_until(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        until = _parse_until(d.pop("until", UNSET))

        connection_activity_response = cls(
            actions=actions,
            agents=agents,
            since=since,
            tool_prefix=tool_prefix,
            last_used_at=last_used_at,
            until=until,
        )

        connection_activity_response.additional_properties = d
        return connection_activity_response

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
