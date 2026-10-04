from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ProfileSummaryView")


@_attrs_define
class ProfileSummaryView:
    """One profile in the list of the profile screen.

    Attributes:
        created_at (str): ISO 8601 creation timestamp.
        id (str): Stable identifier.
        is_default (bool): `true` for the one default profile.
        name (str): Display name of the profile.
        updated_at (str): ISO 8601 last-update timestamp.
        detail (None | str | Unset): A line that tells the profile apart: its role, else its function.
    """

    created_at: str
    id: str
    is_default: bool
    name: str
    updated_at: str
    detail: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at

        id = self.id

        is_default = self.is_default

        name = self.name

        updated_at = self.updated_at

        detail: None | str | Unset
        if isinstance(self.detail, Unset):
            detail = UNSET
        else:
            detail = self.detail

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "created_at": created_at,
                "id": id,
                "is_default": is_default,
                "name": name,
                "updated_at": updated_at,
            }
        )
        if detail is not UNSET:
            field_dict["detail"] = detail

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = d.pop("created_at")

        id = d.pop("id")

        is_default = d.pop("is_default")

        name = d.pop("name")

        updated_at = d.pop("updated_at")

        def _parse_detail(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        detail = _parse_detail(d.pop("detail", UNSET))

        profile_summary_view = cls(
            created_at=created_at,
            id=id,
            is_default=is_default,
            name=name,
            updated_at=updated_at,
            detail=detail,
        )

        profile_summary_view.additional_properties = d
        return profile_summary_view

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
