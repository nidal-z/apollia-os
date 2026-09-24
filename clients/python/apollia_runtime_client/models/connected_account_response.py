from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ConnectedAccountResponse")


@_attrs_define
class ConnectedAccountResponse:
    """Answer of `POST /api/v1/connectors/oauth/complete`.

    Attributes:
        account_id (str): Account identifier: the Google email, or the Microsoft principal name.
        granted_scopes (list[str]): Scopes the provider actually granted.
        provider (str): Provider identifier.
    """

    account_id: str
    granted_scopes: list[str]
    provider: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        account_id = self.account_id

        granted_scopes = self.granted_scopes

        provider = self.provider

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "account_id": account_id,
                "granted_scopes": granted_scopes,
                "provider": provider,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        account_id = d.pop("account_id")

        granted_scopes = cast(list[str], d.pop("granted_scopes"))

        provider = d.pop("provider")

        connected_account_response = cls(
            account_id=account_id,
            granted_scopes=granted_scopes,
            provider=provider,
        )

        connected_account_response.additional_properties = d
        return connected_account_response

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
