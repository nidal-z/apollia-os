from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.offered_access import OfferedAccess
    from ..models.offered_source import OfferedSource


T = TypeVar("T", bound="DraftOffer")


@_attrs_define
class DraftOffer:
    """What a draft may hold: the accesses, the sources and the models that exist
    on the machine when it is read.

        Attributes:
            accesses (list[OfferedAccess]): The tools of the registry, the disabled ones left out, and the code
                context, which a checked code folder grants rather than an access.
            models (list[str]): The models an agent can be assigned, by the name the router routes by.
            sources (list[OfferedSource]): The profile, the memory, then the sources of the registry.
    """

    accesses: list[OfferedAccess]
    models: list[str]
    sources: list[OfferedSource]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        accesses = []
        for accesses_item_data in self.accesses:
            accesses_item = accesses_item_data.to_dict()
            accesses.append(accesses_item)

        models = self.models

        sources = []
        for sources_item_data in self.sources:
            sources_item = sources_item_data.to_dict()
            sources.append(sources_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accesses": accesses,
                "models": models,
                "sources": sources,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.offered_access import OfferedAccess
        from ..models.offered_source import OfferedSource

        d = dict(src_dict)
        accesses = []
        _accesses = d.pop("accesses")
        for accesses_item_data in _accesses:
            accesses_item = OfferedAccess.from_dict(accesses_item_data)

            accesses.append(accesses_item)

        models = cast(list[str], d.pop("models"))

        sources = []
        _sources = d.pop("sources")
        for sources_item_data in _sources:
            sources_item = OfferedSource.from_dict(sources_item_data)

            sources.append(sources_item)

        draft_offer = cls(
            accesses=accesses,
            models=models,
            sources=sources,
        )

        draft_offer.additional_properties = d
        return draft_offer

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
