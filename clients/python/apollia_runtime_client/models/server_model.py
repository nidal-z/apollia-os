from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ServerModel")


@_attrs_define
class ServerModel:
    """One model of a declared server.

    Attributes:
        context_short (bool): Whether that window is shorter than the one an agent is expected to
            have. Nothing is changed on the server: its administrator decides.
        id (str): Identifier the server gives the model.
        route (str): Name to address it by, `<server>/<model>`.
        context_window (int | None | Unset): Context window the server serves the model with, in tokens, read on
            the server (`/api/show` on Ollama, `max_model_len` on vLLM). Absent
            when the server exposes none.
    """

    context_short: bool
    id: str
    route: str
    context_window: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        context_short = self.context_short

        id = self.id

        route = self.route

        context_window: int | None | Unset
        if isinstance(self.context_window, Unset):
            context_window = UNSET
        else:
            context_window = self.context_window

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "context_short": context_short,
                "id": id,
                "route": route,
            }
        )
        if context_window is not UNSET:
            field_dict["context_window"] = context_window

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        context_short = d.pop("context_short")

        id = d.pop("id")

        route = d.pop("route")

        def _parse_context_window(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        context_window = _parse_context_window(d.pop("context_window", UNSET))

        server_model = cls(
            context_short=context_short,
            id=id,
            route=route,
            context_window=context_window,
        )

        server_model.additional_properties = d
        return server_model

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
