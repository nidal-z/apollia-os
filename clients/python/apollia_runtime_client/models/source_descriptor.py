from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.source_descriptor_options import SourceDescriptorOptions
    from ..models.source_write import SourceWrite


T = TypeVar("T", bound="SourceDescriptor")


@_attrs_define
class SourceDescriptor:
    """The source a batch reads its items from.

    Attributes:
        kind (str): The kind of source, as registered (a spreadsheet, a folder, a mailbox).
        location (str): What the source is opened on: a file path, a folder, a connection.
        options (SourceDescriptorOptions | Unset): Options the kind understands, such as a sheet or a header row.
        write (None | SourceWrite | Unset):
    """

    kind: str
    location: str
    options: SourceDescriptorOptions | Unset = UNSET
    write: None | SourceWrite | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.source_write import SourceWrite

        kind = self.kind

        location = self.location

        options: dict[str, Any] | Unset = UNSET
        if not isinstance(self.options, Unset):
            options = self.options.to_dict()

        write: dict[str, Any] | None | Unset
        if isinstance(self.write, Unset):
            write = UNSET
        elif isinstance(self.write, SourceWrite):
            write = self.write.to_dict()
        else:
            write = self.write

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
        if write is not UNSET:
            field_dict["write"] = write

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.source_descriptor_options import SourceDescriptorOptions
        from ..models.source_write import SourceWrite

        d = dict(src_dict)
        kind = d.pop("kind")

        location = d.pop("location")

        _options = d.pop("options", UNSET)
        options: SourceDescriptorOptions | Unset
        if isinstance(_options, Unset):
            options = UNSET
        else:
            options = SourceDescriptorOptions.from_dict(_options)

        def _parse_write(data: object) -> None | SourceWrite | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                write_type_1 = SourceWrite.from_dict(data)

                return write_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | SourceWrite | Unset, data)

        write = _parse_write(d.pop("write", UNSET))

        source_descriptor = cls(
            kind=kind,
            location=location,
            options=options,
            write=write,
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
