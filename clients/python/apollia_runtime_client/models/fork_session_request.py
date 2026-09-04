from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="ForkSessionRequest")



@_attrs_define
class ForkSessionRequest:
    """ Request body for `POST /api/v1/sessions/:id/fork`.

        Attributes:
            up_to_index (int | None | Unset): Number of messages to copy from the parent (None = all).
     """

    up_to_index: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        up_to_index: int | None | Unset
        if isinstance(self.up_to_index, Unset):
            up_to_index = UNSET
        else:
            up_to_index = self.up_to_index


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
        })
        if up_to_index is not UNSET:
            field_dict["up_to_index"] = up_to_index

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        def _parse_up_to_index(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        up_to_index = _parse_up_to_index(d.pop("up_to_index", UNSET))


        fork_session_request = cls(
            up_to_index=up_to_index,
        )


        fork_session_request.additional_properties = d
        return fork_session_request

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
