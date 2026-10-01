from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.server_response import ServerResponse


T = TypeVar("T", bound="ServersListResponse")


@_attrs_define
class ServersListResponse:
    """Body of `GET /api/v1/llm/servers`.

    Attributes:
        servers (list[ServerResponse]): Every declared server, sorted by name.
    """

    servers: list[ServerResponse]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        servers = []
        for servers_item_data in self.servers:
            servers_item = servers_item_data.to_dict()
            servers.append(servers_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "servers": servers,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.server_response import ServerResponse

        d = dict(src_dict)
        servers = []
        _servers = d.pop("servers")
        for servers_item_data in _servers:
            servers_item = ServerResponse.from_dict(servers_item_data)

            servers.append(servers_item)

        servers_list_response = cls(
            servers=servers,
        )

        servers_list_response.additional_properties = d
        return servers_list_response

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
