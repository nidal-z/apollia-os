from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="AgentModelFallbackResponse")


@_attrs_define
class AgentModelFallbackResponse:
    """The last time an agent's model was not available, so a call ran on the
    default model instead.

        Attributes:
            agent (str): Agent name.
            at (str): When it happened, RFC 3339.
            requested (str): Model the agent asked for: the user's assignment, or the backend its
                manifest declares.
            used (str): Model that served the call instead.
    """

    agent: str
    at: str
    requested: str
    used: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent = self.agent

        at = self.at

        requested = self.requested

        used = self.used

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent": agent,
                "at": at,
                "requested": requested,
                "used": used,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent = d.pop("agent")

        at = d.pop("at")

        requested = d.pop("requested")

        used = d.pop("used")

        agent_model_fallback_response = cls(
            agent=agent,
            at=at,
            requested=requested,
            used=used,
        )

        agent_model_fallback_response.additional_properties = d
        return agent_model_fallback_response

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
