from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="FolderErrorBody")


@_attrs_define
class FolderErrorBody:
    """A refused folder: the message and its stable code.

    Attributes:
        code (str): Stable code: `not_a_folder`, `already_added`, `admission_refused`,
            `not_found`, `store`.
        error (str): What went wrong, for a person. For `admission_refused`, the reason
            the admission gave, as is.
    """

    code: str
    error: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code

        error = self.error

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "error": error,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        code = d.pop("code")

        error = d.pop("error")

        folder_error_body = cls(
            code=code,
            error=error,
        )

        folder_error_body.additional_properties = d
        return folder_error_body

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
