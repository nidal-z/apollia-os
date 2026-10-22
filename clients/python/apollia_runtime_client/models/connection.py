from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.connection_kind import ConnectionKind
from ..models.connection_state import ConnectionState
from ..models.connection_status import ConnectionStatus
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.connection_reason_type_0 import ConnectionReasonType0
    from ..models.connection_tool import ConnectionTool


T = TypeVar("T", bound="Connection")


@_attrs_define
class Connection:
    """One connection, whatever its kind.

    Attributes:
        id (str): Stable identifier: `mcp:<server>`, `native:<provider>:<account>`,
            `imap:<address>`, `folder:<id>`.
        kind (ConnectionKind): The kind of thing a connection plugs in.
        label (str): Name shown to the user: the server name, or the provider.
        official (bool): Published by the service's own publisher.
        provenance (str): Where the connection was found before it was added.
        remote (bool): Sends data off this machine. Computed, see [`is_remote`].
        state (ConnectionState): Whether a connection can be used right now, in the detail its kind gives.

            What its user reads is [`Connection::status`], one of four states.
        status (ConnectionStatus): What its user reads: working, needs repair, paused, blocked by the organisation, or not checked yet (nothing has found it in any state).
        tools (list[ConnectionTool]): Tools the connection brings.
        account (None | str | Unset): Account the connection acts as, when it has one.
        checked_at (None | str | Unset): When its state was last found, RFC 3339 in UTC; absent when it never
            was.
        package (None | str | Unset): Name of the installed package that brought it, when one did.
        path (None | str | Unset): Folder of this machine the connection opens, for a folder.
        reason (ConnectionReasonType0 | None | Unset): Why it is not working, absent when it works: a key the interface
            translates, and the technical detail.
    """

    id: str
    kind: ConnectionKind
    label: str
    official: bool
    provenance: str
    remote: bool
    state: ConnectionState
    status: ConnectionStatus
    tools: list[ConnectionTool]
    account: None | str | Unset = UNSET
    checked_at: None | str | Unset = UNSET
    package: None | str | Unset = UNSET
    path: None | str | Unset = UNSET
    reason: ConnectionReasonType0 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.connection_reason_type_0 import ConnectionReasonType0

        id = self.id

        kind = self.kind.value

        label = self.label

        official = self.official

        provenance = self.provenance

        remote = self.remote

        state = self.state.value

        status = self.status.value

        tools = []
        for tools_item_data in self.tools:
            tools_item = tools_item_data.to_dict()
            tools.append(tools_item)

        account: None | str | Unset
        if isinstance(self.account, Unset):
            account = UNSET
        else:
            account = self.account

        checked_at: None | str | Unset
        if isinstance(self.checked_at, Unset):
            checked_at = UNSET
        else:
            checked_at = self.checked_at

        package: None | str | Unset
        if isinstance(self.package, Unset):
            package = UNSET
        else:
            package = self.package

        path: None | str | Unset
        if isinstance(self.path, Unset):
            path = UNSET
        else:
            path = self.path

        reason: dict[str, Any] | None | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        elif isinstance(self.reason, ConnectionReasonType0):
            reason = self.reason.to_dict()
        else:
            reason = self.reason

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
                "status": status,
                "tools": tools,
            }
        )
        if account is not UNSET:
            field_dict["account"] = account
        if checked_at is not UNSET:
            field_dict["checkedAt"] = checked_at
        if package is not UNSET:
            field_dict["package"] = package
        if path is not UNSET:
            field_dict["path"] = path
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.connection_reason_type_0 import ConnectionReasonType0
        from ..models.connection_tool import ConnectionTool

        d = dict(src_dict)
        id = d.pop("id")

        kind = ConnectionKind(d.pop("kind"))

        label = d.pop("label")

        official = d.pop("official")

        provenance = d.pop("provenance")

        remote = d.pop("remote")

        state = ConnectionState(d.pop("state"))

        status = ConnectionStatus(d.pop("status"))

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

        def _parse_checked_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        checked_at = _parse_checked_at(d.pop("checkedAt", UNSET))

        def _parse_package(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        package = _parse_package(d.pop("package", UNSET))

        def _parse_path(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        path = _parse_path(d.pop("path", UNSET))

        def _parse_reason(data: object) -> ConnectionReasonType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                reason_type_0 = ConnectionReasonType0.from_dict(data)

                return reason_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(ConnectionReasonType0 | None | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))

        connection = cls(
            id=id,
            kind=kind,
            label=label,
            official=official,
            provenance=provenance,
            remote=remote,
            state=state,
            status=status,
            tools=tools,
            account=account,
            checked_at=checked_at,
            package=package,
            path=path,
            reason=reason,
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
