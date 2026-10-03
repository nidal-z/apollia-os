from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CreatedServerTokenView")


@_attrs_define
class CreatedServerTokenView:
    """Answer of `POST /api/v1/server/tokens`: the token and, this once, its value.

    Attributes:
        created_at (str): When it was created, RFC 3339.
        id (int): Identifier, usable to revoke it.
        last_four (str): The last four characters of the value.
        name (str): Name given at creation.
        revoked (bool): Whether it was revoked.
        value (str): The value (`apo_` and 64 hexadecimal characters). Shown this once.
        last_used_at (None | str | Unset): When it last opened the server, RFC 3339; absent if never.
        revoked_at (None | str | Unset): When it was revoked, RFC 3339.
    """

    created_at: str
    id: int
    last_four: str
    name: str
    revoked: bool
    value: str
    last_used_at: None | str | Unset = UNSET
    revoked_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at

        id = self.id

        last_four = self.last_four

        name = self.name

        revoked = self.revoked

        value = self.value

        last_used_at: None | str | Unset
        if isinstance(self.last_used_at, Unset):
            last_used_at = UNSET
        else:
            last_used_at = self.last_used_at

        revoked_at: None | str | Unset
        if isinstance(self.revoked_at, Unset):
            revoked_at = UNSET
        else:
            revoked_at = self.revoked_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "createdAt": created_at,
                "id": id,
                "lastFour": last_four,
                "name": name,
                "revoked": revoked,
                "value": value,
            }
        )
        if last_used_at is not UNSET:
            field_dict["lastUsedAt"] = last_used_at
        if revoked_at is not UNSET:
            field_dict["revokedAt"] = revoked_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = d.pop("createdAt")

        id = d.pop("id")

        last_four = d.pop("lastFour")

        name = d.pop("name")

        revoked = d.pop("revoked")

        value = d.pop("value")

        def _parse_last_used_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_used_at = _parse_last_used_at(d.pop("lastUsedAt", UNSET))

        def _parse_revoked_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        revoked_at = _parse_revoked_at(d.pop("revokedAt", UNSET))

        created_server_token_view = cls(
            created_at=created_at,
            id=id,
            last_four=last_four,
            name=name,
            revoked=revoked,
            value=value,
            last_used_at=last_used_at,
            revoked_at=revoked_at,
        )

        created_server_token_view.additional_properties = d
        return created_server_token_view

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
