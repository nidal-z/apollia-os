from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.dependent_source import DependentSource


T = TypeVar("T", bound="FolderErrorBody")


@_attrs_define
class FolderErrorBody:
    """A refused folder: the message and its stable code.

    Attributes:
        code (str): Stable code: `not_a_folder`, `already_added`, `admission_refused`,
            `not_found`, `has_dependent_sources`, `sources`, `store`.
        error (str): What went wrong, for a person. For `admission_refused`, the reason
            the admission gave, as is.
        dependent_sources (list[DependentSource] | Unset): For `has_dependent_sources`, the sources removing the folder
            would
            remove; absent otherwise.
    """

    code: str
    error: str
    dependent_sources: list[DependentSource] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code

        error = self.error

        dependent_sources: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.dependent_sources, Unset):
            dependent_sources = []
            for dependent_sources_item_data in self.dependent_sources:
                dependent_sources_item = dependent_sources_item_data.to_dict()
                dependent_sources.append(dependent_sources_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "error": error,
            }
        )
        if dependent_sources is not UNSET:
            field_dict["dependent_sources"] = dependent_sources

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.dependent_source import DependentSource

        d = dict(src_dict)
        code = d.pop("code")

        error = d.pop("error")

        _dependent_sources = d.pop("dependent_sources", UNSET)
        dependent_sources: list[DependentSource] | Unset = UNSET
        if _dependent_sources is not UNSET:
            dependent_sources = []
            for dependent_sources_item_data in _dependent_sources:
                dependent_sources_item = DependentSource.from_dict(dependent_sources_item_data)

                dependent_sources.append(dependent_sources_item)

        folder_error_body = cls(
            code=code,
            error=error,
            dependent_sources=dependent_sources,
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
