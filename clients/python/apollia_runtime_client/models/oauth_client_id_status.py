from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="OauthClientIdStatus")


@_attrs_define
class OauthClientIdStatus:
    """One provider's OAuth client, as the settings draw it.

    Reports the **effective** client id of the resolution chain
    (`env var → override file → compiled default`), where it came from, and
    whether a client secret and an API key resolve. Never carries a secret or
    a key, only their presence.

        Attributes:
            api_key_source (str): Source for the resolved API key. Same semantics as `source`.
            client_secret_source (str): Same source semantics as `source` but for the secret.
            effective_client_id (str): Effective client id used at runtime. Empty string when no source is configured.
            has_api_key (bool): True when an API key resolves through any source. Google Picker needs
                one; Microsoft entries leave this false.
            has_client_secret (bool): True when a client secret resolves through any source.
            provider (str): Provider id (`"google"` / `"microsoft"`).
            requires_api_key (bool): True when this provider needs an API key (Google, for Picker only).
            requires_client_secret (bool): True when this provider needs a client secret (Google), false for a
                public client (Microsoft).
            source (str): Source that produced the effective value: `"env"`, `"file"`, `"builtin"`, or `"none"`.
            override_client_id (None | str | Unset): Override stored in `oauth-clients.toml`, if any.
    """

    api_key_source: str
    client_secret_source: str
    effective_client_id: str
    has_api_key: bool
    has_client_secret: bool
    provider: str
    requires_api_key: bool
    requires_client_secret: bool
    source: str
    override_client_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        api_key_source = self.api_key_source

        client_secret_source = self.client_secret_source

        effective_client_id = self.effective_client_id

        has_api_key = self.has_api_key

        has_client_secret = self.has_client_secret

        provider = self.provider

        requires_api_key = self.requires_api_key

        requires_client_secret = self.requires_client_secret

        source = self.source

        override_client_id: None | str | Unset
        if isinstance(self.override_client_id, Unset):
            override_client_id = UNSET
        else:
            override_client_id = self.override_client_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "api_key_source": api_key_source,
                "client_secret_source": client_secret_source,
                "effective_client_id": effective_client_id,
                "has_api_key": has_api_key,
                "has_client_secret": has_client_secret,
                "provider": provider,
                "requires_api_key": requires_api_key,
                "requires_client_secret": requires_client_secret,
                "source": source,
            }
        )
        if override_client_id is not UNSET:
            field_dict["override_client_id"] = override_client_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        api_key_source = d.pop("api_key_source")

        client_secret_source = d.pop("client_secret_source")

        effective_client_id = d.pop("effective_client_id")

        has_api_key = d.pop("has_api_key")

        has_client_secret = d.pop("has_client_secret")

        provider = d.pop("provider")

        requires_api_key = d.pop("requires_api_key")

        requires_client_secret = d.pop("requires_client_secret")

        source = d.pop("source")

        def _parse_override_client_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        override_client_id = _parse_override_client_id(d.pop("override_client_id", UNSET))

        oauth_client_id_status = cls(
            api_key_source=api_key_source,
            client_secret_source=client_secret_source,
            effective_client_id=effective_client_id,
            has_api_key=has_api_key,
            has_client_secret=has_client_secret,
            provider=provider,
            requires_api_key=requires_api_key,
            requires_client_secret=requires_client_secret,
            source=source,
            override_client_id=override_client_id,
        )

        oauth_client_id_status.additional_properties = d
        return oauth_client_id_status

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
