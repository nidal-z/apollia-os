from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="OpenLeaseRequest")


@_attrs_define
class OpenLeaseRequest:
    """Request body for `POST /api/v1/llm/leases`.

    Attributes:
        client_id (str): The interface holding the conversation, the `client` it passes to
            `GET /api/v1/events`.
        conversation_id (str): The conversation on screen.
        model_path (str): Path of the model the conversation designates, as the resident list
            reports it.
    """

    client_id: str
    conversation_id: str
    model_path: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        client_id = self.client_id

        conversation_id = self.conversation_id

        model_path = self.model_path

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "clientId": client_id,
                "conversationId": conversation_id,
                "modelPath": model_path,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        client_id = d.pop("clientId")

        conversation_id = d.pop("conversationId")

        model_path = d.pop("modelPath")

        open_lease_request = cls(
            client_id=client_id,
            conversation_id=conversation_id,
            model_path=model_path,
        )

        open_lease_request.additional_properties = d
        return open_lease_request

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
