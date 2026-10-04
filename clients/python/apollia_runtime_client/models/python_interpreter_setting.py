from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="PythonInterpreterSetting")


@_attrs_define
class PythonInterpreterSetting:
    """The Python interpreter setting.

    Attributes:
        required_minor (int): Minor version the bundled standard library requires of any chosen
            interpreter.
        bundled (None | str | Unset): Absolute path of the interpreter this runtime ships, when it knows it;
            `None` in a development tree, where no bundle is staged.
        chosen (None | str | Unset): The absolute path in `[tools] python_interpreter`, or `None` when the
            bundled interpreter is in use.
    """

    required_minor: int
    bundled: None | str | Unset = UNSET
    chosen: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        required_minor = self.required_minor

        bundled: None | str | Unset
        if isinstance(self.bundled, Unset):
            bundled = UNSET
        else:
            bundled = self.bundled

        chosen: None | str | Unset
        if isinstance(self.chosen, Unset):
            chosen = UNSET
        else:
            chosen = self.chosen

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "required_minor": required_minor,
            }
        )
        if bundled is not UNSET:
            field_dict["bundled"] = bundled
        if chosen is not UNSET:
            field_dict["chosen"] = chosen

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        required_minor = d.pop("required_minor")

        def _parse_bundled(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        bundled = _parse_bundled(d.pop("bundled", UNSET))

        def _parse_chosen(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        chosen = _parse_chosen(d.pop("chosen", UNSET))

        python_interpreter_setting = cls(
            required_minor=required_minor,
            bundled=bundled,
            chosen=chosen,
        )

        python_interpreter_setting.additional_properties = d
        return python_interpreter_setting

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
