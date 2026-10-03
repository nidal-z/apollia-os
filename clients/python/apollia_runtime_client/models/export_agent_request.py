from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ExportAgentRequest")


@_attrs_define
class ExportAgentRequest:
    """Body of `POST /api/v1/agent-definitions/{name}/export`.

    Attributes:
        publisher (None | str | Unset): Publisher name for this export only; the `author.name` setting when
            absent or blank.
        sign (bool | Unset): Sign the package with the author key, generated on the first signature.
    """

    publisher: None | str | Unset = UNSET
    sign: bool | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        publisher: None | str | Unset
        if isinstance(self.publisher, Unset):
            publisher = UNSET
        else:
            publisher = self.publisher

        sign = self.sign

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if publisher is not UNSET:
            field_dict["publisher"] = publisher
        if sign is not UNSET:
            field_dict["sign"] = sign

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)

        def _parse_publisher(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        publisher = _parse_publisher(d.pop("publisher", UNSET))

        sign = d.pop("sign", UNSET)

        export_agent_request = cls(
            publisher=publisher,
            sign=sign,
        )

        export_agent_request.additional_properties = d
        return export_agent_request

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
