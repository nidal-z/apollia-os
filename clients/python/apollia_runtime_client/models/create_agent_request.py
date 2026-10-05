from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CreateAgentRequest")


@_attrs_define
class CreateAgentRequest:
    """Body of `POST /api/v1/agent-definitions`.

    Attributes:
        mission (str): What the agent is for, used as its instructions.
        name (str): Display name. The identifier of the agent is derived from it.
        access (list[str] | None | Unset): Accesses granted. Absent, the agent reads by default, as any agent
            created without code; a list, even empty, replaces that default.
        examples (list[str] | Unset): Example requests.
        sources (list[str] | Unset): Sources checked: identifiers of `GET /api/v1/sources`, `profile` or
            `memory`.
    """

    mission: str
    name: str
    access: list[str] | None | Unset = UNSET
    examples: list[str] | Unset = UNSET
    sources: list[str] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        mission = self.mission

        name = self.name

        access: list[str] | None | Unset
        if isinstance(self.access, Unset):
            access = UNSET
        elif isinstance(self.access, list):
            access = self.access

        else:
            access = self.access

        examples: list[str] | Unset = UNSET
        if not isinstance(self.examples, Unset):
            examples = self.examples

        sources: list[str] | Unset = UNSET
        if not isinstance(self.sources, Unset):
            sources = self.sources

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "mission": mission,
                "name": name,
            }
        )
        if access is not UNSET:
            field_dict["access"] = access
        if examples is not UNSET:
            field_dict["examples"] = examples
        if sources is not UNSET:
            field_dict["sources"] = sources

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        mission = d.pop("mission")

        name = d.pop("name")

        def _parse_access(data: object) -> list[str] | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                access_type_0 = cast(list[str], data)

                return access_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(list[str] | None | Unset, data)

        access = _parse_access(d.pop("access", UNSET))

        examples = cast(list[str], d.pop("examples", UNSET))

        sources = cast(list[str], d.pop("sources", UNSET))

        create_agent_request = cls(
            mission=mission,
            name=name,
            access=access,
            examples=examples,
            sources=sources,
        )

        create_agent_request.additional_properties = d
        return create_agent_request

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
