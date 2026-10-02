from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.added_access import AddedAccess


T = TypeVar("T", bound="UpdateDiff")


@_attrs_define
class UpdateDiff:
    """What an update changes, against the installed version.

    Attributes:
        added_access (list[AddedAccess]): Accesses the incoming package defaults grant that the installed ones
            did not.
        added_mcp_servers (list[str]): MCP servers brought by the incoming version only.
        added_requires (list[str]): Connection types required by the incoming version only.
        downgrade (bool): The incoming version is older than the installed one.
        from_version (str): Installed version.
        to_version (str): Incoming version.
    """

    added_access: list[AddedAccess]
    added_mcp_servers: list[str]
    added_requires: list[str]
    downgrade: bool
    from_version: str
    to_version: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        added_access = []
        for added_access_item_data in self.added_access:
            added_access_item = added_access_item_data.to_dict()
            added_access.append(added_access_item)

        added_mcp_servers = self.added_mcp_servers

        added_requires = self.added_requires

        downgrade = self.downgrade

        from_version = self.from_version

        to_version = self.to_version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "addedAccess": added_access,
                "addedMcpServers": added_mcp_servers,
                "addedRequires": added_requires,
                "downgrade": downgrade,
                "fromVersion": from_version,
                "toVersion": to_version,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.added_access import AddedAccess

        d = dict(src_dict)
        added_access = []
        _added_access = d.pop("addedAccess")
        for added_access_item_data in _added_access:
            added_access_item = AddedAccess.from_dict(added_access_item_data)

            added_access.append(added_access_item)

        added_mcp_servers = cast(list[str], d.pop("addedMcpServers"))

        added_requires = cast(list[str], d.pop("addedRequires"))

        downgrade = d.pop("downgrade")

        from_version = d.pop("fromVersion")

        to_version = d.pop("toVersion")

        update_diff = cls(
            added_access=added_access,
            added_mcp_servers=added_mcp_servers,
            added_requires=added_requires,
            downgrade=downgrade,
            from_version=from_version,
            to_version=to_version,
        )

        update_diff.additional_properties = d
        return update_diff

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
