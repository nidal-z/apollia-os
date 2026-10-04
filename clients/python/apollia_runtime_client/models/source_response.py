from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.index_state_dto import IndexStateDto
from ..models.source_kind_dto import SourceKindDto
from ..types import UNSET, Unset

T = TypeVar("T", bound="SourceResponse")


@_attrs_define
class SourceResponse:
    """A source of the registry.

    Attributes:
        added_at (str): When it was added, RFC 3339 in UTC.
        id (str): Identifier, the form an agent's `sources` list names it by.
        index_state (IndexStateDto): Where the index of a source stands.
        kind (SourceKindDto): What a source is.
        label (str): The name the user sees.
        group (None | str | Unset): The group the source belongs to, if any.
        reference (None | str | Unset): What the source points at: a folder connection, a file path, a mail
            connection, a memory space.
    """

    added_at: str
    id: str
    index_state: IndexStateDto
    kind: SourceKindDto
    label: str
    group: None | str | Unset = UNSET
    reference: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        added_at = self.added_at

        id = self.id

        index_state = self.index_state.value

        kind = self.kind.value

        label = self.label

        group: None | str | Unset
        if isinstance(self.group, Unset):
            group = UNSET
        else:
            group = self.group

        reference: None | str | Unset
        if isinstance(self.reference, Unset):
            reference = UNSET
        else:
            reference = self.reference

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "addedAt": added_at,
                "id": id,
                "indexState": index_state,
                "kind": kind,
                "label": label,
            }
        )
        if group is not UNSET:
            field_dict["group"] = group
        if reference is not UNSET:
            field_dict["reference"] = reference

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        added_at = d.pop("addedAt")

        id = d.pop("id")

        index_state = IndexStateDto(d.pop("indexState"))

        kind = SourceKindDto(d.pop("kind"))

        label = d.pop("label")

        def _parse_group(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        group = _parse_group(d.pop("group", UNSET))

        def _parse_reference(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reference = _parse_reference(d.pop("reference", UNSET))

        source_response = cls(
            added_at=added_at,
            id=id,
            index_state=index_state,
            kind=kind,
            label=label,
            group=group,
            reference=reference,
        )

        source_response.additional_properties = d
        return source_response

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
