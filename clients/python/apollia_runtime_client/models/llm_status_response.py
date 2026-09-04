from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.llm_status_response_backends_item import LlmStatusResponseBackendsItem





T = TypeVar("T", bound="LlmStatusResponse")



@_attrs_define
class LlmStatusResponse:
    """ Response body for `GET /api/v1/llm/status`.

        Attributes:
            backends (list[LlmStatusResponseBackendsItem]): All configured LLM backends with their current availability
                state.
            ceiling_reached (bool): Whether the cost ceiling has been reached. Always `false` when hybrid
                routing is not configured.
            ceiling_usd (float | None | Unset): Hybrid routing cost ceiling in USD. `None` when hybrid routing is not
                configured.
            cost_usd (float | None | Unset): Accumulated session cost in USD. `None` when no router is configured.
     """

    backends: list[LlmStatusResponseBackendsItem]
    ceiling_reached: bool
    ceiling_usd: float | None | Unset = UNSET
    cost_usd: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.llm_status_response_backends_item import LlmStatusResponseBackendsItem
        backends = []
        for backends_item_data in self.backends:
            backends_item = backends_item_data.to_dict()
            backends.append(backends_item)



        ceiling_reached = self.ceiling_reached

        ceiling_usd: float | None | Unset
        if isinstance(self.ceiling_usd, Unset):
            ceiling_usd = UNSET
        else:
            ceiling_usd = self.ceiling_usd

        cost_usd: float | None | Unset
        if isinstance(self.cost_usd, Unset):
            cost_usd = UNSET
        else:
            cost_usd = self.cost_usd


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "backends": backends,
            "ceiling_reached": ceiling_reached,
        })
        if ceiling_usd is not UNSET:
            field_dict["ceiling_usd"] = ceiling_usd
        if cost_usd is not UNSET:
            field_dict["cost_usd"] = cost_usd

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.llm_status_response_backends_item import LlmStatusResponseBackendsItem
        d = dict(src_dict)
        backends = []
        _backends = d.pop("backends")
        for backends_item_data in (_backends):
            backends_item = LlmStatusResponseBackendsItem.from_dict(backends_item_data)



            backends.append(backends_item)


        ceiling_reached = d.pop("ceiling_reached")

        def _parse_ceiling_usd(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        ceiling_usd = _parse_ceiling_usd(d.pop("ceiling_usd", UNSET))


        def _parse_cost_usd(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        cost_usd = _parse_cost_usd(d.pop("cost_usd", UNSET))


        llm_status_response = cls(
            backends=backends,
            ceiling_reached=ceiling_reached,
            ceiling_usd=ceiling_usd,
            cost_usd=cost_usd,
        )


        llm_status_response.additional_properties = d
        return llm_status_response

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
