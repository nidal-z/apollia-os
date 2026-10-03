from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="McpExposure")


@_attrs_define
class McpExposure:
    """Body of `PUT /api/v1/mcp/exposed`.

    Attributes:
        exposed (bool): Whether the bridge serves the item.
        item_id (str): The agent name, or `<package>/<workflow>`.
        kind (str): `agent` or `workflow`.
    """

    exposed: bool
    item_id: str
    kind: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        exposed = self.exposed

        item_id = self.item_id

        kind = self.kind

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "exposed": exposed,
                "itemId": item_id,
                "kind": kind,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        exposed = d.pop("exposed")

        item_id = d.pop("itemId")

        kind = d.pop("kind")

        mcp_exposure = cls(
            exposed=exposed,
            item_id=item_id,
            kind=kind,
        )

        mcp_exposure.additional_properties = d
        return mcp_exposure

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
