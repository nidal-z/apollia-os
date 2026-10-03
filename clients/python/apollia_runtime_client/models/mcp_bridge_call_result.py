from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="McpBridgeCallResult")


@_attrs_define
class McpBridgeCallResult:
    """The end of a bridge call.

    Attributes:
        is_error (bool): `true` when the run failed or was stopped.
        item_id (str): The agent name, or `<package>/<workflow>`.
        kind (str): `agent` or `workflow`.
        status (str): `completed`, `failed` or `canceled`.
        text (str): The answer of the agent, or what the workflow returned; the failure
            when the run did not complete.
    """

    is_error: bool
    item_id: str
    kind: str
    status: str
    text: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        is_error = self.is_error

        item_id = self.item_id

        kind = self.kind

        status = self.status

        text = self.text

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "isError": is_error,
                "itemId": item_id,
                "kind": kind,
                "status": status,
                "text": text,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        is_error = d.pop("isError")

        item_id = d.pop("itemId")

        kind = d.pop("kind")

        status = d.pop("status")

        text = d.pop("text")

        mcp_bridge_call_result = cls(
            is_error=is_error,
            item_id=item_id,
            kind=kind,
            status=status,
            text=text,
        )

        mcp_bridge_call_result.additional_properties = d
        return mcp_bridge_call_result

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
