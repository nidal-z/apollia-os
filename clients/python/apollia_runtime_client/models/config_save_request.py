from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.declared_origin import DeclaredOrigin
from ..types import UNSET, Unset

T = TypeVar("T", bound="ConfigSaveRequest")


@_attrs_define
class ConfigSaveRequest:
    """A text to save, with the fingerprint of the text it was made from.

    Attributes:
        sha256 (str): SHA-256 of the text the change was made from, as `GET` returned it.
        text (str): The whole text of the new `apollia.toml`.
        origin (DeclaredOrigin | Unset): The origin a caller of the save route may declare. The two origins the
            runtime finds by itself are not among them.
    """

    sha256: str
    text: str
    origin: DeclaredOrigin | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        sha256 = self.sha256

        text = self.text

        origin: str | Unset = UNSET
        if not isinstance(self.origin, Unset):
            origin = self.origin.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "sha256": sha256,
                "text": text,
            }
        )
        if origin is not UNSET:
            field_dict["origin"] = origin

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        sha256 = d.pop("sha256")

        text = d.pop("text")

        _origin = d.pop("origin", UNSET)
        origin: DeclaredOrigin | Unset
        if isinstance(_origin, Unset):
            origin = UNSET
        else:
            origin = DeclaredOrigin(_origin)

        config_save_request = cls(
            sha256=sha256,
            text=text,
            origin=origin,
        )

        config_save_request.additional_properties = d
        return config_save_request

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
