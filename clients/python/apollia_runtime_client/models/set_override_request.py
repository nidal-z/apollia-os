from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.set_override_request_value import SetOverrideRequestValue


T = TypeVar("T", bound="SetOverrideRequest")


@_attrs_define
class SetOverrideRequest:
    """Body of `PUT /api/v1/agent-definitions/{name}/overrides/{field}`.

    Attributes:
        value (SetOverrideRequestValue): The user's value: a string for `name` and `mission`, a tier name for
            `autonomy`, a profile identifier for `profile`, a list of strings for `examples`, `access` and `sources`.
            A `sources` entry is a source identifier of `GET /api/v1/sources`, `profile` or `memory`.
    """

    value: SetOverrideRequestValue
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        value = self.value.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "value": value,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.set_override_request_value import SetOverrideRequestValue

        d = dict(src_dict)
        value = SetOverrideRequestValue.from_dict(d.pop("value"))

        set_override_request = cls(
            value=value,
        )

        set_override_request.additional_properties = d
        return set_override_request

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
