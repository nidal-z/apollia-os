from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.message_dto import MessageDto





T = TypeVar("T", bound="CompleteRequest")



@_attrs_define
class CompleteRequest:
    """ Request body for `POST /api/v1/llm/complete`.

        Attributes:
            messages (list[MessageDto]): Ordered list of messages forming the conversation history.
            backend (None | str | Unset): Backend to use; falls back to the router default if omitted.
            grammar (None | str | Unset): Optional GBNF grammar constraining the decoding (local backends).
     """

    messages: list[MessageDto]
    backend: None | str | Unset = UNSET
    grammar: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.message_dto import MessageDto
        messages = []
        for messages_item_data in self.messages:
            messages_item = messages_item_data.to_dict()
            messages.append(messages_item)



        backend: None | str | Unset
        if isinstance(self.backend, Unset):
            backend = UNSET
        else:
            backend = self.backend

        grammar: None | str | Unset
        if isinstance(self.grammar, Unset):
            grammar = UNSET
        else:
            grammar = self.grammar


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "messages": messages,
        })
        if backend is not UNSET:
            field_dict["backend"] = backend
        if grammar is not UNSET:
            field_dict["grammar"] = grammar

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.message_dto import MessageDto
        d = dict(src_dict)
        messages = []
        _messages = d.pop("messages")
        for messages_item_data in (_messages):
            messages_item = MessageDto.from_dict(messages_item_data)



            messages.append(messages_item)


        def _parse_backend(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        backend = _parse_backend(d.pop("backend", UNSET))


        def _parse_grammar(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        grammar = _parse_grammar(d.pop("grammar", UNSET))


        complete_request = cls(
            messages=messages,
            backend=backend,
            grammar=grammar,
        )


        complete_request.additional_properties = d
        return complete_request

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
