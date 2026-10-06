from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.skip_reason_dto import SkipReasonDto

T = TypeVar("T", bound="SkippedFileResponse")


@_attrs_define
class SkippedFileResponse:
    """A file left out of an index.

    Attributes:
        path (str): Path of the file on this machine.
        reason (SkipReasonDto): Why a file was left out of an index.
        size_bytes (int): Its size in bytes when it was left out.
    """

    path: str
    reason: SkipReasonDto
    size_bytes: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        path = self.path

        reason = self.reason.value

        size_bytes = self.size_bytes

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "path": path,
                "reason": reason,
                "sizeBytes": size_bytes,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        path = d.pop("path")

        reason = SkipReasonDto(d.pop("reason"))

        size_bytes = d.pop("sizeBytes")

        skipped_file_response = cls(
            path=path,
            reason=reason,
            size_bytes=size_bytes,
        )

        skipped_file_response.additional_properties = d
        return skipped_file_response

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
