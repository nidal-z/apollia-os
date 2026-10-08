from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.model_cards_response_cards_item import ModelCardsResponseCardsItem
    from ..models.model_cards_response_publishers import ModelCardsResponsePublishers


T = TypeVar("T", bound="ModelCardsResponse")


@_attrs_define
class ModelCardsResponse:
    """The catalogue's cards, or why there are none.

    Attributes:
        cards (list[ModelCardsResponseCardsItem]): Every card, in the asked language.
        publishers (ModelCardsResponsePublishers): The official publishers and the recognised converters.
        status (str): `ok`, `offline` (nothing answered and no copy is kept), `disabled`
            (turned off by configuration or policy) or `no_trusted_key` (this build
            carries no distribution key).
        message (None | str | Unset): A sentence for the user when the catalogue is empty.
        origin (None | str | Unset): Where the index came from when `status` is `ok`: `primary`, `mirror`,
            `cache` or `shipped`.
    """

    cards: list[ModelCardsResponseCardsItem]
    publishers: ModelCardsResponsePublishers
    status: str
    message: None | str | Unset = UNSET
    origin: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        cards = []
        for cards_item_data in self.cards:
            cards_item = cards_item_data.to_dict()
            cards.append(cards_item)

        publishers = self.publishers.to_dict()

        status = self.status

        message: None | str | Unset
        if isinstance(self.message, Unset):
            message = UNSET
        else:
            message = self.message

        origin: None | str | Unset
        if isinstance(self.origin, Unset):
            origin = UNSET
        else:
            origin = self.origin

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "cards": cards,
                "publishers": publishers,
                "status": status,
            }
        )
        if message is not UNSET:
            field_dict["message"] = message
        if origin is not UNSET:
            field_dict["origin"] = origin

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.model_cards_response_cards_item import ModelCardsResponseCardsItem
        from ..models.model_cards_response_publishers import ModelCardsResponsePublishers

        d = dict(src_dict)
        cards = []
        _cards = d.pop("cards")
        for cards_item_data in _cards:
            cards_item = ModelCardsResponseCardsItem.from_dict(cards_item_data)

            cards.append(cards_item)

        publishers = ModelCardsResponsePublishers.from_dict(d.pop("publishers"))

        status = d.pop("status")

        def _parse_message(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        message = _parse_message(d.pop("message", UNSET))

        def _parse_origin(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        origin = _parse_origin(d.pop("origin", UNSET))

        model_cards_response = cls(
            cards=cards,
            publishers=publishers,
            status=status,
            message=message,
            origin=origin,
        )

        model_cards_response.additional_properties = d
        return model_cards_response

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
