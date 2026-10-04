from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.signature_status import SignatureStatus
from ..types import UNSET, Unset

T = TypeVar("T", bound="SignaturePreview")


@_attrs_define
class SignaturePreview:
    """The publisher and the signature, as the preview shows them.

    Attributes:
        declared_publisher (str): Publisher name the package declares. Not verified.
        publisher (str): Name to show for the publisher: the label of the keyring when the key
            is trusted, otherwise the name the package declares.
        status (SignatureStatus): State of the signature of a package.
        trusted (bool): The signing key is in the keyring.
        key_fingerprint (None | str | Unset): SHA-256 fingerprint of the signing key, when the signature holds.
        public_key (None | str | Unset): The signing public key, 64 hexadecimal characters, when the signature
            holds: what trusting the publisher imports into the keyring.
        reason (None | str | Unset): Why the signature does not hold, when it does not.
    """

    declared_publisher: str
    publisher: str
    status: SignatureStatus
    trusted: bool
    key_fingerprint: None | str | Unset = UNSET
    public_key: None | str | Unset = UNSET
    reason: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        declared_publisher = self.declared_publisher

        publisher = self.publisher

        status = self.status.value

        trusted = self.trusted

        key_fingerprint: None | str | Unset
        if isinstance(self.key_fingerprint, Unset):
            key_fingerprint = UNSET
        else:
            key_fingerprint = self.key_fingerprint

        public_key: None | str | Unset
        if isinstance(self.public_key, Unset):
            public_key = UNSET
        else:
            public_key = self.public_key

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "declaredPublisher": declared_publisher,
                "publisher": publisher,
                "status": status,
                "trusted": trusted,
            }
        )
        if key_fingerprint is not UNSET:
            field_dict["keyFingerprint"] = key_fingerprint
        if public_key is not UNSET:
            field_dict["publicKey"] = public_key
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        declared_publisher = d.pop("declaredPublisher")

        publisher = d.pop("publisher")

        status = SignatureStatus(d.pop("status"))

        trusted = d.pop("trusted")

        def _parse_key_fingerprint(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        key_fingerprint = _parse_key_fingerprint(d.pop("keyFingerprint", UNSET))

        def _parse_public_key(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        public_key = _parse_public_key(d.pop("publicKey", UNSET))

        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))

        signature_preview = cls(
            declared_publisher=declared_publisher,
            publisher=publisher,
            status=status,
            trusted=trusted,
            key_fingerprint=key_fingerprint,
            public_key=public_key,
            reason=reason,
        )

        signature_preview.additional_properties = d
        return signature_preview

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
