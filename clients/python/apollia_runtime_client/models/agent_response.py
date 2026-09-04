from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.agent_response_manifest_type_0 import AgentResponseManifestType0
  from ..models.skill_dto import SkillDto





T = TypeVar("T", bound="AgentResponse")



@_attrs_define
class AgentResponse:
    """ Response body for agent operations.

        Attributes:
            agent_id (str): Agent identifier (UUID v4).
            skills (list[SkillDto]): Skills declared by this agent (populated in list and detail responses).
            state (str): Current process state as string.
            supports_a2a (bool): Whether this agent supports A2A inter-agent communication.
            manifest (AgentResponseManifestType0 | None | Unset): Agent manifest (present in detail view).
            name (None | str | Unset): Agent name from manifest (always present).
            version (None | str | Unset): Agent version from manifest.
     """

    agent_id: str
    skills: list[SkillDto]
    state: str
    supports_a2a: bool
    manifest: AgentResponseManifestType0 | None | Unset = UNSET
    name: None | str | Unset = UNSET
    version: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.agent_response_manifest_type_0 import AgentResponseManifestType0
        from ..models.skill_dto import SkillDto
        agent_id = self.agent_id

        skills = []
        for skills_item_data in self.skills:
            skills_item = skills_item_data.to_dict()
            skills.append(skills_item)



        state = self.state

        supports_a2a = self.supports_a2a

        manifest: dict[str, Any] | None | Unset
        if isinstance(self.manifest, Unset):
            manifest = UNSET
        elif isinstance(self.manifest, AgentResponseManifestType0):
            manifest = self.manifest.to_dict()
        else:
            manifest = self.manifest

        name: None | str | Unset
        if isinstance(self.name, Unset):
            name = UNSET
        else:
            name = self.name

        version: None | str | Unset
        if isinstance(self.version, Unset):
            version = UNSET
        else:
            version = self.version


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agent_id": agent_id,
            "skills": skills,
            "state": state,
            "supports_a2a": supports_a2a,
        })
        if manifest is not UNSET:
            field_dict["manifest"] = manifest
        if name is not UNSET:
            field_dict["name"] = name
        if version is not UNSET:
            field_dict["version"] = version

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_response_manifest_type_0 import AgentResponseManifestType0
        from ..models.skill_dto import SkillDto
        d = dict(src_dict)
        agent_id = d.pop("agent_id")

        skills = []
        _skills = d.pop("skills")
        for skills_item_data in (_skills):
            skills_item = SkillDto.from_dict(skills_item_data)



            skills.append(skills_item)


        state = d.pop("state")

        supports_a2a = d.pop("supports_a2a")

        def _parse_manifest(data: object) -> AgentResponseManifestType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                manifest_type_0 = AgentResponseManifestType0.from_dict(data)



                return manifest_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(AgentResponseManifestType0 | None | Unset, data)

        manifest = _parse_manifest(d.pop("manifest", UNSET))


        def _parse_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        name = _parse_name(d.pop("name", UNSET))


        def _parse_version(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        version = _parse_version(d.pop("version", UNSET))


        agent_response = cls(
            agent_id=agent_id,
            skills=skills,
            state=state,
            supports_a2a=supports_a2a,
            manifest=manifest,
            name=name,
            version=version,
        )


        agent_response.additional_properties = d
        return agent_response

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
