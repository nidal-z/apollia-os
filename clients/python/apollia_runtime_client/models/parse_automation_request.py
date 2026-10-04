from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ParseAutomationRequest")


@_attrs_define
class ParseAutomationRequest:
    """An automation described in words, and the agents the window knows.

    Attributes:
        input_ (str): Free-form text entered by the operator.
        known_agents (list[str] | Unset): Identifiers or names of the agents known on the window side.
    """

    input_: str
    known_agents: list[str] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        input_ = self.input_

        known_agents: list[str] | Unset = UNSET
        if not isinstance(self.known_agents, Unset):
            known_agents = self.known_agents

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "input": input_,
            }
        )
        if known_agents is not UNSET:
            field_dict["known_agents"] = known_agents

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        input_ = d.pop("input")

        known_agents = cast(list[str], d.pop("known_agents", UNSET))

        parse_automation_request = cls(
            input_=input_,
            known_agents=known_agents,
        )

        parse_automation_request.additional_properties = d
        return parse_automation_request

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
