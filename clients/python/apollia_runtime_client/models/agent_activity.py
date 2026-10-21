from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AgentActivity")


@_attrs_define
class AgentActivity:
    """What one agent did with the tools of a connection over the period.

    Attributes:
        actions (int): Tool calls of the connection that ended over the period.
        last_used_at (str): When the last of them ended, RFC 3339 UTC.
        agent (None | str | Unset): The agent, `None` for calls whose entry names none.
    """

    actions: int
    last_used_at: str
    agent: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        actions = self.actions

        last_used_at = self.last_used_at

        agent: None | str | Unset
        if isinstance(self.agent, Unset):
            agent = UNSET
        else:
            agent = self.agent

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "actions": actions,
                "lastUsedAt": last_used_at,
            }
        )
        if agent is not UNSET:
            field_dict["agent"] = agent

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        actions = d.pop("actions")

        last_used_at = d.pop("lastUsedAt")

        def _parse_agent(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent = _parse_agent(d.pop("agent", UNSET))

        agent_activity = cls(
            actions=actions,
            last_used_at=last_used_at,
            agent=agent,
        )

        agent_activity.additional_properties = d
        return agent_activity

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
