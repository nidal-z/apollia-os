from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.connection_kind import ConnectionKind
from ..models.connection_state import ConnectionState
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.connection_tool import ConnectionTool


T = TypeVar("T", bound="Connection")


@_attrs_define
class Connection:
    """One connection, whatever its kind.

    Attributes:
        id (str): Stable identifier: `mcp:<server>`, `native:<provider>:<account>`.
        kind (ConnectionKind): The kind of thing a connection plugs in.
        label (str): Name shown to the user: the server name, or the provider.
        official (bool): Published by the service's own publisher.
        provenance (str): Where the connection was found before it was added.
        remote (bool): Sends data off this machine. Computed, see [`is_remote`].
        state (ConnectionState): Whether a connection can be used right now.
        tools (list[ConnectionTool]): Tools the connection brings.
        account (None | str | Unset): Account the connection acts as, when it has one.
    """

    id: str
    kind: ConnectionKind
    label: str
    official: bool
    provenance: str
    remote: bool
    state: ConnectionState
    tools: list[ConnectionTool]
    account: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        id = self.id

        kind = self.kind.value

        label = self.label

        official = self.official

        provenance = self.provenance

        remote = self.remote

        state = self.state.value

        tools = []
        for tools_item_data in self.tools:
            tools_item = tools_item_data.to_dict()
            tools.append(tools_item)

        account: None | str | Unset
        if isinstance(self.account, Unset):
            account = UNSET
        else:
            account = self.account

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "id": id,
                "kind": kind,
                "label": label,
                "official": official,
                "provenance": provenance,
                "remote": remote,
                "state": state,
                "tools": tools,
            }
        )
        if account is not UNSET:
            field_dict["account"] = account

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.connection_tool import ConnectionTool

        d = dict(src_dict)
        id = d.pop("id")

        kind = ConnectionKind(d.pop("kind"))

        label = d.pop("label")

        official = d.pop("official")

        provenance = d.pop("provenance")

        remote = d.pop("remote")

        state = ConnectionState(d.pop("state"))

        tools = []
        _tools = d.pop("tools")
        for tools_item_data in _tools:
            tools_item = ConnectionTool.from_dict(tools_item_data)

            tools.append(tools_item)

        def _parse_account(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        account = _parse_account(d.pop("account", UNSET))

        connection = cls(
            id=id,
            kind=kind,
            label=label,
            official=official,
            provenance=provenance,
            remote=remote,
            state=state,
            tools=tools,
            account=account,
        )

        connection.additional_properties = d
        return connection

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
