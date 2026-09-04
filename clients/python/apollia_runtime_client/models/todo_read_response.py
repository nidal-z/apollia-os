from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.todo_read_response_items_item import TodoReadResponseItemsItem





T = TypeVar("T", bound="TodoReadResponse")



@_attrs_define
class TodoReadResponse:
    """ Response body for `GET /api/v1/sessions/:id/todo`.

        Attributes:
            items (list[TodoReadResponseItemsItem]): Current todo items, ordered by insertion.
            session_id (str): Session whose todo list is returned.
     """

    items: list[TodoReadResponseItemsItem]
    session_id: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.todo_read_response_items_item import TodoReadResponseItemsItem
        items = []
        for items_item_data in self.items:
            items_item = items_item_data.to_dict()
            items.append(items_item)



        session_id = self.session_id


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "items": items,
            "session_id": session_id,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.todo_read_response_items_item import TodoReadResponseItemsItem
        d = dict(src_dict)
        items = []
        _items = d.pop("items")
        for items_item_data in (_items):
            items_item = TodoReadResponseItemsItem.from_dict(items_item_data)



            items.append(items_item)


        session_id = d.pop("session_id")

        todo_read_response = cls(
            items=items,
            session_id=session_id,
        )


        todo_read_response.additional_properties = d
        return todo_read_response

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
