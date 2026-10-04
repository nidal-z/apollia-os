from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AgentPackageSignature")


@_attrs_define
class AgentPackageSignature:
    """Who published an installed package, as recorded at its installation.

    Attributes:
        publisher (str): Name to show for the publisher: the label of the keyring when the key
            that signed the package is trusted, otherwise the name the package
            declares, which nothing verifies.
        signed (bool): The package carried a signature that held.
        trusted (bool): The key that signed it is in the keyring of trusted publishers.
        key_fingerprint (None | str | Unset): SHA-256 fingerprint of the signing key, when the package is signed.
    """

    publisher: str
    signed: bool
    trusted: bool
    key_fingerprint: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        publisher = self.publisher

        signed = self.signed

        trusted = self.trusted

        key_fingerprint: None | str | Unset
        if isinstance(self.key_fingerprint, Unset):
            key_fingerprint = UNSET
        else:
            key_fingerprint = self.key_fingerprint

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "publisher": publisher,
                "signed": signed,
                "trusted": trusted,
            }
        )
        if key_fingerprint is not UNSET:
            field_dict["key_fingerprint"] = key_fingerprint

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        publisher = d.pop("publisher")

        signed = d.pop("signed")

        trusted = d.pop("trusted")

        def _parse_key_fingerprint(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        key_fingerprint = _parse_key_fingerprint(d.pop("key_fingerprint", UNSET))

        agent_package_signature = cls(
            publisher=publisher,
            signed=signed,
            trusted=trusted,
            key_fingerprint=key_fingerprint,
        )

        agent_package_signature.additional_properties = d
        return agent_package_signature

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
