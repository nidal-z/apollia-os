from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="SetupLlmBody")


@_attrs_define
class SetupLlmBody:
    """Request body for `POST /api/v1/local-ai/llm`.

    Attributes:
        gguf_path (str): Absolute path of the GGUF file chosen.
        context_window (int | None | Unset): The context window chosen, in tokens; absent keeps the stored one.
    """

    gguf_path: str
    context_window: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        gguf_path = self.gguf_path

        context_window: int | None | Unset
        if isinstance(self.context_window, Unset):
            context_window = UNSET
        else:
            context_window = self.context_window

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "ggufPath": gguf_path,
            }
        )
        if context_window is not UNSET:
            field_dict["contextWindow"] = context_window

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        gguf_path = d.pop("ggufPath")

        def _parse_context_window(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        context_window = _parse_context_window(d.pop("contextWindow", UNSET))

        setup_llm_body = cls(
            gguf_path=gguf_path,
            context_window=context_window,
        )

        setup_llm_body.additional_properties = d
        return setup_llm_body

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
