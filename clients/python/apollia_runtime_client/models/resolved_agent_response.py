from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.agent_origin_dto import AgentOriginDto
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.agent_model_response import AgentModelResponse
    from ..models.resolved_agent_response_definition import ResolvedAgentResponseDefinition


T = TypeVar("T", bound="ResolvedAgentResponse")


@_attrs_define
class ResolvedAgentResponse:
    """An agent in force: its definition, then the user's overrides.

    Attributes:
        access (list[str]): Tools granted.
        autonomy (str): Autonomy tier the agent runs at: `assisted`, `supervised`,
            `bounded_autonomous` or `long_autonomous`. The user's, else the
            package's, else the global default.
        definition (ResolvedAgentResponseDefinition): The definition as the package declares it, before any override.
        enabled (bool): Whether the agent is active.
        examples (list[str]): Example requests.
        id (str): Identifier.
        mission (str): What the agent is for, used as its instructions.
        name (str): Display name.
        origin (AgentOriginDto): Where an agent comes from.
        overridden (list[str]): Fields whose value is the user's rather than the package's:
            `name`, `mission`, `examples`, `access`, `access_added`, `sources`,
            `autonomy`, `planning`, `profile`. `access_added` names the tools
            given on top of the default access, which `access` lists after it.
        planning (str): Whether the agent plans a task before acting on it: `never`, `auto` or
            `always`. The user's, else the package's, else `auto`.
        sources (list[str]): Sources checked.
        version (str): Version of the definition.
        model (AgentModelResponse | None | Unset):
        package (None | str | Unset): Package the agent came with.
        profile (None | str | Unset): Identifier of the user profile designated for the agent; absent when
            it runs with the default profile.
    """

    access: list[str]
    autonomy: str
    definition: ResolvedAgentResponseDefinition
    enabled: bool
    examples: list[str]
    id: str
    mission: str
    name: str
    origin: AgentOriginDto
    overridden: list[str]
    planning: str
    sources: list[str]
    version: str
    model: AgentModelResponse | None | Unset = UNSET
    package: None | str | Unset = UNSET
    profile: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.agent_model_response import AgentModelResponse

        access = self.access

        autonomy = self.autonomy

        definition = self.definition.to_dict()

        enabled = self.enabled

        examples = self.examples

        id = self.id

        mission = self.mission

        name = self.name

        origin = self.origin.value

        overridden = self.overridden

        planning = self.planning

        sources = self.sources

        version = self.version

        model: dict[str, Any] | None | Unset
        if isinstance(self.model, Unset):
            model = UNSET
        elif isinstance(self.model, AgentModelResponse):
            model = self.model.to_dict()
        else:
            model = self.model

        package: None | str | Unset
        if isinstance(self.package, Unset):
            package = UNSET
        else:
            package = self.package

        profile: None | str | Unset
        if isinstance(self.profile, Unset):
            profile = UNSET
        else:
            profile = self.profile

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "access": access,
                "autonomy": autonomy,
                "definition": definition,
                "enabled": enabled,
                "examples": examples,
                "id": id,
                "mission": mission,
                "name": name,
                "origin": origin,
                "overridden": overridden,
                "planning": planning,
                "sources": sources,
                "version": version,
            }
        )
        if model is not UNSET:
            field_dict["model"] = model
        if package is not UNSET:
            field_dict["package"] = package
        if profile is not UNSET:
            field_dict["profile"] = profile

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_model_response import AgentModelResponse
        from ..models.resolved_agent_response_definition import ResolvedAgentResponseDefinition

        d = dict(src_dict)
        access = cast(list[str], d.pop("access"))

        autonomy = d.pop("autonomy")

        definition = ResolvedAgentResponseDefinition.from_dict(d.pop("definition"))

        enabled = d.pop("enabled")

        examples = cast(list[str], d.pop("examples"))

        id = d.pop("id")

        mission = d.pop("mission")

        name = d.pop("name")

        origin = AgentOriginDto(d.pop("origin"))

        overridden = cast(list[str], d.pop("overridden"))

        planning = d.pop("planning")

        sources = cast(list[str], d.pop("sources"))

        version = d.pop("version")

        def _parse_model(data: object) -> AgentModelResponse | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                model_type_1 = AgentModelResponse.from_dict(data)

                return model_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(AgentModelResponse | None | Unset, data)

        model = _parse_model(d.pop("model", UNSET))

        def _parse_package(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        package = _parse_package(d.pop("package", UNSET))

        def _parse_profile(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        profile = _parse_profile(d.pop("profile", UNSET))

        resolved_agent_response = cls(
            access=access,
            autonomy=autonomy,
            definition=definition,
            enabled=enabled,
            examples=examples,
            id=id,
            mission=mission,
            name=name,
            origin=origin,
            overridden=overridden,
            planning=planning,
            sources=sources,
            version=version,
            model=model,
            package=package,
            profile=profile,
        )

        resolved_agent_response.additional_properties = d
        return resolved_agent_response

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
