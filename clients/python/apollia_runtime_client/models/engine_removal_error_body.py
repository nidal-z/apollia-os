from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="EngineRemovalErrorBody")


@_attrs_define
class EngineRemovalErrorBody:
    """A refused removal: the message and its stable code.

    Attributes:
        code (str): Stable code: `not_found`, `in_service`, `embedded`, `chosen`,
            `locked`, `config`, `store`.
        error (str): What went wrong, for a person.
        after (None | str | Unset): With `chosen`, what `engines.gguf` comes down to once the build is
            gone: `auto`, or the tag of the build of the chosen variant that
            takes its place.
    """

    code: str
    error: str
    after: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code

        error = self.error

        after: None | str | Unset
        if isinstance(self.after, Unset):
            after = UNSET
        else:
            after = self.after

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "error": error,
            }
        )
        if after is not UNSET:
            field_dict["after"] = after

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        code = d.pop("code")

        error = d.pop("error")

        def _parse_after(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        after = _parse_after(d.pop("after", UNSET))

        engine_removal_error_body = cls(
            code=code,
            error=error,
            after=after,
        )

        engine_removal_error_body.additional_properties = d
        return engine_removal_error_body

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
