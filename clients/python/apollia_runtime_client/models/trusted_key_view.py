from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="TrustedKeyView")


@_attrs_define
class TrustedKeyView:
    """A trusted publisher key.

    Attributes:
        added_at (str): When it was added.
        fingerprint (str): SHA-256 fingerprint of the key.
        label (str): Name shown for the publisher.
        origin (str): Who put the key there: `user` or `policy`.
        public_key (str): The public key, 64 hexadecimal characters.
    """

    added_at: str
    fingerprint: str
    label: str
    origin: str
    public_key: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        added_at = self.added_at

        fingerprint = self.fingerprint

        label = self.label

        origin = self.origin

        public_key = self.public_key

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "addedAt": added_at,
                "fingerprint": fingerprint,
                "label": label,
                "origin": origin,
                "publicKey": public_key,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        added_at = d.pop("addedAt")

        fingerprint = d.pop("fingerprint")

        label = d.pop("label")

        origin = d.pop("origin")

        public_key = d.pop("publicKey")

        trusted_key_view = cls(
            added_at=added_at,
            fingerprint=fingerprint,
            label=label,
            origin=origin,
            public_key=public_key,
        )

        trusted_key_view.additional_properties = d
        return trusted_key_view

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
