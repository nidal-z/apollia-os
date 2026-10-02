from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.install_refusal_type_3_code import InstallRefusalType3Code
from ..types import UNSET, Unset

T = TypeVar("T", bound="InstallRefusalType3")


@_attrs_define
class InstallRefusalType3:
    """An agent of the package has the identifier of an agent installed
    otherwise.

        Attributes:
            agent (str): The identifier.
            code (InstallRefusalType3Code):
            owner (None | str | Unset): The package that owns it, `None` for an agent created in the
                interface.
    """

    agent: str
    code: InstallRefusalType3Code
    owner: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent = self.agent

        code = self.code.value

        owner: None | str | Unset
        if isinstance(self.owner, Unset):
            owner = UNSET
        else:
            owner = self.owner

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent": agent,
                "code": code,
            }
        )
        if owner is not UNSET:
            field_dict["owner"] = owner

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent = d.pop("agent")

        code = InstallRefusalType3Code(d.pop("code"))

        def _parse_owner(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        owner = _parse_owner(d.pop("owner", UNSET))

        install_refusal_type_3 = cls(
            agent=agent,
            code=code,
            owner=owner,
        )

        install_refusal_type_3.additional_properties = d
        return install_refusal_type_3

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
