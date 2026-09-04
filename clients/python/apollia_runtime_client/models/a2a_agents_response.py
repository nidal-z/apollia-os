from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.a2a_agent_dto import A2AAgentDto





T = TypeVar("T", bound="A2AAgentsResponse")



@_attrs_define
class A2AAgentsResponse:
    """ Response body for `GET /api/v1/a2a/agents`.

        Attributes:
            agents (list[A2AAgentDto]): Active A2A agents with their skills.
     """

    agents: list[A2AAgentDto]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.a2a_agent_dto import A2AAgentDto
        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agents": agents,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.a2a_agent_dto import A2AAgentDto
        d = dict(src_dict)
        agents = []
        _agents = d.pop("agents")
        for agents_item_data in (_agents):
            agents_item = A2AAgentDto.from_dict(agents_item_data)



            agents.append(agents_item)


        a2a_agents_response = cls(
            agents=agents,
        )


        a2a_agents_response.additional_properties = d
        return a2a_agents_response

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
