from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="PermissionRuleDto")


@_attrs_define
class PermissionRuleDto:
    """One permission rule, as the permissions page draws it.

    Attributes:
        action (str): Applied action (`"allow"` or `"deny"`).
        created_at (str): ISO 8601 creation date.
        id (int): Identifier of the persisted rule.
        scope (str): Rule scope (`"agent"`, `"global"`).
        tool_name (str): Name of the targeted tool.
        agent_id (None | str | Unset): Agent identifier, for `agent` rules.
        arg_prefix (None | str | Unset): Optional argument prefix.
        created_by (None | str | Unset): Author of the rule (`None` means a human operator).
        expires_at (None | str | Unset): ISO 8601 expiration date, if any.
    """

    action: str
    created_at: str
    id: int
    scope: str
    tool_name: str
    agent_id: None | str | Unset = UNSET
    arg_prefix: None | str | Unset = UNSET
    created_by: None | str | Unset = UNSET
    expires_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        action = self.action

        created_at = self.created_at

        id = self.id

        scope = self.scope

        tool_name = self.tool_name

        agent_id: None | str | Unset
        if isinstance(self.agent_id, Unset):
            agent_id = UNSET
        else:
            agent_id = self.agent_id

        arg_prefix: None | str | Unset
        if isinstance(self.arg_prefix, Unset):
            arg_prefix = UNSET
        else:
            arg_prefix = self.arg_prefix

        created_by: None | str | Unset
        if isinstance(self.created_by, Unset):
            created_by = UNSET
        else:
            created_by = self.created_by

        expires_at: None | str | Unset
        if isinstance(self.expires_at, Unset):
            expires_at = UNSET
        else:
            expires_at = self.expires_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "action": action,
                "created_at": created_at,
                "id": id,
                "scope": scope,
                "tool_name": tool_name,
            }
        )
        if agent_id is not UNSET:
            field_dict["agent_id"] = agent_id
        if arg_prefix is not UNSET:
            field_dict["arg_prefix"] = arg_prefix
        if created_by is not UNSET:
            field_dict["created_by"] = created_by
        if expires_at is not UNSET:
            field_dict["expires_at"] = expires_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        action = d.pop("action")

        created_at = d.pop("created_at")

        id = d.pop("id")

        scope = d.pop("scope")

        tool_name = d.pop("tool_name")

        def _parse_agent_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent_id = _parse_agent_id(d.pop("agent_id", UNSET))

        def _parse_arg_prefix(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        arg_prefix = _parse_arg_prefix(d.pop("arg_prefix", UNSET))

        def _parse_created_by(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        created_by = _parse_created_by(d.pop("created_by", UNSET))

        def _parse_expires_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        expires_at = _parse_expires_at(d.pop("expires_at", UNSET))

        permission_rule_dto = cls(
            action=action,
            created_at=created_at,
            id=id,
            scope=scope,
            tool_name=tool_name,
            agent_id=agent_id,
            arg_prefix=arg_prefix,
            created_by=created_by,
            expires_at=expires_at,
        )

        permission_rule_dto.additional_properties = d
        return permission_rule_dto

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
