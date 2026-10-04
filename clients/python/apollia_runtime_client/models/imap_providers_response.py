from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.imap_provider_response import ImapProviderResponse


T = TypeVar("T", bound="ImapProvidersResponse")


@_attrs_define
class ImapProvidersResponse:
    """The providers known from the address, and the port of any other server.

    Attributes:
        default_port (int): Port of a server outside the list, implicit TLS.
        providers (list[ImapProviderResponse]): Every provider prefilled from the address.
    """

    default_port: int
    providers: list[ImapProviderResponse]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        default_port = self.default_port

        providers = []
        for providers_item_data in self.providers:
            providers_item = providers_item_data.to_dict()
            providers.append(providers_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "defaultPort": default_port,
                "providers": providers,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.imap_provider_response import ImapProviderResponse

        d = dict(src_dict)
        default_port = d.pop("defaultPort")

        providers = []
        _providers = d.pop("providers")
        for providers_item_data in _providers:
            providers_item = ImapProviderResponse.from_dict(providers_item_data)

            providers.append(providers_item)

        imap_providers_response = cls(
            default_port=default_port,
            providers=providers,
        )

        imap_providers_response.additional_properties = d
        return imap_providers_response

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
