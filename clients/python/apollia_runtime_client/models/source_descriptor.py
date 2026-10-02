from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.source_descriptor_options import SourceDescriptorOptions


T = TypeVar("T", bound="SourceDescriptor")


@_attrs_define
class SourceDescriptor:
    """The source a batch reads its items from.

    Attributes:
        kind (str): The kind of source, as registered (a spreadsheet, a folder, a mailbox).
        location (str): What the source is opened on: a file path, a folder, a connection.
        options (SourceDescriptorOptions | Unset): Options the kind understands, such as a sheet or a header row.
    """

    kind: str
    location: str
    options: SourceDescriptorOptions | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        kind = self.kind

        location = self.location

        options: dict[str, Any] | Unset = UNSET
        if not isinstance(self.options, Unset):
            options = self.options.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "kind": kind,
                "location": location,
            }
        )
        if options is not UNSET:
            field_dict["options"] = options

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.source_descriptor_options import SourceDescriptorOptions

        d = dict(src_dict)
        kind = d.pop("kind")

        location = d.pop("location")

        _options = d.pop("options", UNSET)
        options: SourceDescriptorOptions | Unset
        if isinstance(_options, Unset):
            options = UNSET
        else:
            options = SourceDescriptorOptions.from_dict(_options)

        source_descriptor = cls(
            kind=kind,
            location=location,
            options=options,
        )

        source_descriptor.additional_properties = d
        return source_descriptor

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
