from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="StartConsentBody")


@_attrs_define
class StartConsentBody:
    """Body of `POST /api/v1/connectors/oauth/start`.

    Attributes:
        provider (str): Provider identifier: `google` or `microsoft`.
        redirect_uri (str): Page the provider returns the user to, registered with the provider
            under the OAuth client the runtime resolves. `https`, or `http` on a
            loopback host.
        scopes (list[str] | Unset): Scope names from the provider's catalogue (`mail.send`,
            `calendar.read`, ...). The identity scopes are added implicitly.
    """

    provider: str
    redirect_uri: str
    scopes: list[str] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        provider = self.provider

        redirect_uri = self.redirect_uri

        scopes: list[str] | Unset = UNSET
        if not isinstance(self.scopes, Unset):
            scopes = self.scopes

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "provider": provider,
                "redirect_uri": redirect_uri,
            }
        )
        if scopes is not UNSET:
            field_dict["scopes"] = scopes

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        provider = d.pop("provider")

        redirect_uri = d.pop("redirect_uri")

        scopes = cast(list[str], d.pop("scopes", UNSET))

        start_consent_body = cls(
            provider=provider,
            redirect_uri=redirect_uri,
            scopes=scopes,
        )

        start_consent_body.additional_properties = d
        return start_consent_body

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
