from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ReleaseLeaseResponse")


@_attrs_define
class ReleaseLeaseResponse:
    """Response body for `POST /api/v1/llm/leases/release`.

    Attributes:
        unloaded (bool): Whether that model was unloaded. It stays loaded when pinned or
            answering a request.
        released (None | str | Unset): The model no open conversation designates any more; absent when the
            lease did not exist or another conversation still holds the model.
    """

    unloaded: bool
    released: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        unloaded = self.unloaded

        released: None | str | Unset
        if isinstance(self.released, Unset):
            released = UNSET
        else:
            released = self.released

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "unloaded": unloaded,
            }
        )
        if released is not UNSET:
            field_dict["released"] = released

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        unloaded = d.pop("unloaded")

        def _parse_released(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        released = _parse_released(d.pop("released", UNSET))

        release_lease_response = cls(
            unloaded=unloaded,
            released=released,
        )

        release_lease_response.additional_properties = d
        return release_lease_response

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
