from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.token_usage_response import TokenUsageResponse





T = TypeVar("T", bound="ChatResponse")



@_attrs_define
class ChatResponse:
    """ Response body for `POST /api/v1/llm/chat` and `POST /api/v1/llm/complete`.

        Attributes:
            backend (str): Name of the backend that served this request (for transparency).
            content (str): LLM-generated response text.
            latency_ms (int): Total round-trip latency in milliseconds.
            usage (TokenUsageResponse): Token usage included in chat responses.
     """

    backend: str
    content: str
    latency_ms: int
    usage: TokenUsageResponse
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.token_usage_response import TokenUsageResponse
        backend = self.backend

        content = self.content

        latency_ms = self.latency_ms

        usage = self.usage.to_dict()


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "backend": backend,
            "content": content,
            "latency_ms": latency_ms,
            "usage": usage,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.token_usage_response import TokenUsageResponse
        d = dict(src_dict)
        backend = d.pop("backend")

        content = d.pop("content")

        latency_ms = d.pop("latency_ms")

        usage = TokenUsageResponse.from_dict(d.pop("usage"))




        chat_response = cls(
            backend=backend,
            content=content,
            latency_ms=latency_ms,
            usage=usage,
        )


        chat_response.additional_properties = d
        return chat_response

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
