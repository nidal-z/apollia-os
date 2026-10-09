from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.draft_access import DraftAccess


T = TypeVar("T", bound="AgentDraft")


@_attrs_define
class AgentDraft:
    """The draft of an agent, as the form opens it. Nothing of it is stored.

    Attributes:
        access (list[DraftAccess]): The accesses proposed, then the reading family, each one existing on
            the machine.
        examples (list[str]): Example requests.
        mission (str): What the agent is for, used as its instructions.
        name (str): Display name.
        sources (list[str]): The sources proposed, each one existing on the machine.
        unavailable (list[str]): What the sheet asked for and the machine does not hold: accesses,
            sources (`source:<id>`) and a model (`model:<name>`).
        model (None | str | Unset): The model proposed, `None` for the default model of the machine.
    """

    access: list[DraftAccess]
    examples: list[str]
    mission: str
    name: str
    sources: list[str]
    unavailable: list[str]
    model: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        access = []
        for access_item_data in self.access:
            access_item = access_item_data.to_dict()
            access.append(access_item)

        examples = self.examples

        mission = self.mission

        name = self.name

        sources = self.sources

        unavailable = self.unavailable

        model: None | str | Unset
        if isinstance(self.model, Unset):
            model = UNSET
        else:
            model = self.model

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "access": access,
                "examples": examples,
                "mission": mission,
                "name": name,
                "sources": sources,
                "unavailable": unavailable,
            }
        )
        if model is not UNSET:
            field_dict["model"] = model

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.draft_access import DraftAccess

        d = dict(src_dict)
        access = []
        _access = d.pop("access")
        for access_item_data in _access:
            access_item = DraftAccess.from_dict(access_item_data)

            access.append(access_item)

        examples = cast(list[str], d.pop("examples"))

        mission = d.pop("mission")

        name = d.pop("name")

        sources = cast(list[str], d.pop("sources"))

        unavailable = cast(list[str], d.pop("unavailable"))

        def _parse_model(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        model = _parse_model(d.pop("model", UNSET))

        agent_draft = cls(
            access=access,
            examples=examples,
            mission=mission,
            name=name,
            sources=sources,
            unavailable=unavailable,
            model=model,
        )

        agent_draft.additional_properties = d
        return agent_draft

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
