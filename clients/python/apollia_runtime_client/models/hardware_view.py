from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.hardware_card import HardwareCard


T = TypeVar("T", bound="HardwareView")


@_attrs_define
class HardwareView:
    """The cards of the machine and what is installed around them.

    Attributes:
        cards (list[HardwareCard]): Every card, NVIDIA ones first.
        rocm_runtime (bool): Whether the host's ROCm runtime is installed. Linux only.
        vulkan_loader (bool): Whether a Vulkan loader is installed.
    """

    cards: list[HardwareCard]
    rocm_runtime: bool
    vulkan_loader: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        cards = []
        for cards_item_data in self.cards:
            cards_item = cards_item_data.to_dict()
            cards.append(cards_item)

        rocm_runtime = self.rocm_runtime

        vulkan_loader = self.vulkan_loader

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "cards": cards,
                "rocm_runtime": rocm_runtime,
                "vulkan_loader": vulkan_loader,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.hardware_card import HardwareCard

        d = dict(src_dict)
        cards = []
        _cards = d.pop("cards")
        for cards_item_data in _cards:
            cards_item = HardwareCard.from_dict(cards_item_data)

            cards.append(cards_item)

        rocm_runtime = d.pop("rocm_runtime")

        vulkan_loader = d.pop("vulkan_loader")

        hardware_view = cls(
            cards=cards,
            rocm_runtime=rocm_runtime,
            vulkan_loader=vulkan_loader,
        )

        hardware_view.additional_properties = d
        return hardware_view

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
