from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ProposedRuleView")


@_attrs_define
class ProposedRuleView:
    """One proposed permission rule, as the approval card draws it.

    Attributes:
        action (str): `allow` or `deny`.
        index (int): Position in the persisted list, the cursor for apply and dismiss.
        scope (str): `global` or `agent`.
        tool_name (str): Tool the rule targets.
        arg_prefix (None | str | Unset): Argument prefix the rule matches, when it has one.
    """

    action: str
    index: int
    scope: str
    tool_name: str
    arg_prefix: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        action = self.action

        index = self.index

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
                "index": index,
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

        index = d.pop("index")

        scope = d.pop("scope")

        tool_name = d.pop("tool_name")

        def _parse_arg_prefix(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        arg_prefix = _parse_arg_prefix(d.pop("arg_prefix", UNSET))

        proposed_rule_view = cls(
            action=action,
            index=index,
            scope=scope,
            tool_name=tool_name,
            arg_prefix=arg_prefix,
        )

        proposed_rule_view.additional_properties = d
        return proposed_rule_view

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
