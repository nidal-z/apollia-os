from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.import_memory_body_content import ImportMemoryBodyContent


T = TypeVar("T", bound="ImportMemoryBody")


@_attrs_define
class ImportMemoryBody:
    """Request body for `POST /api/v1/memory/namespaces/{namespace}/import`.

    Attributes:
        content (ImportMemoryBodyContent): The export, as `GET .../export` answers it.
        mode (str): `replace` empties the namespace first; any other value merges.
    """

    content: ImportMemoryBodyContent
    mode: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        content = self.content.to_dict()

        mode = self.mode

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "content": content,
                "mode": mode,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.import_memory_body_content import ImportMemoryBodyContent

        d = dict(src_dict)
        content = ImportMemoryBodyContent.from_dict(d.pop("content"))

        mode = d.pop("mode")

        import_memory_body = cls(
            content=content,
            mode=mode,
        )

        import_memory_body.additional_properties = d
        return import_memory_body

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
