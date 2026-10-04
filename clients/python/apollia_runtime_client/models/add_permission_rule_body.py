from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AddPermissionRuleBody")


@_attrs_define
class AddPermissionRuleBody:
    """Request body for `POST /api/v1/permissions/rules`.

    Attributes:
        action (str): `allow` or `deny`.
        scope (str): `global`, the only scope this route writes.
        tool_name (str): Tool the rule targets.
        arg_prefix (None | str | Unset): Argument prefix the rule matches; absent matches every call.
    """

    action: str
    scope: str
    tool_name: str
    arg_prefix: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        action = self.action

        scope = self.scope

        tool_name = self.tool_name

        arg_prefix: None | str | Unset
        if isinstance(self.arg_prefix, Unset):
            arg_prefix = UNSET
        else:
            arg_prefix = self.arg_prefix

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "action": action,
                "scope": scope,
                "tool_name": tool_name,
            }
        )
        if arg_prefix is not UNSET:
            field_dict["arg_prefix"] = arg_prefix

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        action = d.pop("action")

        scope = d.pop("scope")

        tool_name = d.pop("tool_name")

        def _parse_arg_prefix(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        arg_prefix = _parse_arg_prefix(d.pop("arg_prefix", UNSET))

        add_permission_rule_body = cls(
            action=action,
            scope=scope,
            tool_name=tool_name,
            arg_prefix=arg_prefix,
        )

        add_permission_rule_body.additional_properties = d
        return add_permission_rule_body

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
