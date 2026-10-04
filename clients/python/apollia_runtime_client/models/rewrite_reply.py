from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.rewrite_fallback_reason import RewriteFallbackReason
from ..types import UNSET, Unset

T = TypeVar("T", bound="RewriteReply")


@_attrs_define
class RewriteReply:
    """The rewritten prompt, or the original one with the reason it came back.

    Attributes:
        rewritten_text (str): The text to put back in the composer.
        fallback (None | RewriteFallbackReason | Unset):
    """

    rewritten_text: str
    fallback: None | RewriteFallbackReason | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        rewritten_text = self.rewritten_text

        fallback: None | str | Unset
        if isinstance(self.fallback, Unset):
            fallback = UNSET
        elif isinstance(self.fallback, RewriteFallbackReason):
            fallback = self.fallback.value
        else:
            fallback = self.fallback

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "rewrittenText": rewritten_text,
            }
        )
        if fallback is not UNSET:
            field_dict["fallback"] = fallback

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        rewritten_text = d.pop("rewrittenText")

        def _parse_fallback(data: object) -> None | RewriteFallbackReason | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                fallback_type_1 = RewriteFallbackReason(data)

                return fallback_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | RewriteFallbackReason | Unset, data)

        fallback = _parse_fallback(d.pop("fallback", UNSET))

        rewrite_reply = cls(
            rewritten_text=rewritten_text,
            fallback=fallback,
        )

        rewrite_reply.additional_properties = d
        return rewrite_reply

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
