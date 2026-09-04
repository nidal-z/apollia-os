from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.resilience_status_response_circuit_breakers_item import ResilienceStatusResponseCircuitBreakersItem





T = TypeVar("T", bound="ResilienceStatusResponse")



@_attrs_define
class ResilienceStatusResponse:
    """ Envelope for `GET /api/v1/resilience/status`.

        Attributes:
            circuit_breakers (list[ResilienceStatusResponseCircuitBreakersItem]): Snapshot for every tool the runtime has
                seen at least one event for.
     """

    circuit_breakers: list[ResilienceStatusResponseCircuitBreakersItem]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.resilience_status_response_circuit_breakers_item import ResilienceStatusResponseCircuitBreakersItem
        circuit_breakers = []
        for circuit_breakers_item_data in self.circuit_breakers:
            circuit_breakers_item = circuit_breakers_item_data.to_dict()
            circuit_breakers.append(circuit_breakers_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "circuit_breakers": circuit_breakers,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.resilience_status_response_circuit_breakers_item import ResilienceStatusResponseCircuitBreakersItem
        d = dict(src_dict)
        circuit_breakers = []
        _circuit_breakers = d.pop("circuit_breakers")
        for circuit_breakers_item_data in (_circuit_breakers):
            circuit_breakers_item = ResilienceStatusResponseCircuitBreakersItem.from_dict(circuit_breakers_item_data)



            circuit_breakers.append(circuit_breakers_item)


        resilience_status_response = cls(
            circuit_breakers=circuit_breakers,
        )


        resilience_status_response.additional_properties = d
        return resilience_status_response

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
