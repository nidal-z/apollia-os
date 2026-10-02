from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="BroughtServerPreview")


@_attrs_define
class BroughtServerPreview:
    """An MCP server the package brings.

    Attributes:
        already_present (bool): A connection of that name already exists; it is left as it is.
        command (str): Executable spawned, for `stdio`.
        name (str): Server name.
        remote (bool): The server sends data off this machine.
        transport (str): `stdio`, `streamable-http` or `sse`.
        url (None | str | Unset): Address, for a network transport.
    """

    already_present: bool
    command: str
    name: str
    remote: bool
    transport: str
    url: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        already_present = self.already_present

        command = self.command

        name = self.name

        remote = self.remote

        transport = self.transport

        url: None | str | Unset
        if isinstance(self.url, Unset):
            url = UNSET
        else:
            url = self.url

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "alreadyPresent": already_present,
                "command": command,
                "name": name,
                "remote": remote,
                "transport": transport,
            }
        )
        if url is not UNSET:
            field_dict["url"] = url

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        already_present = d.pop("alreadyPresent")

        command = d.pop("command")

        name = d.pop("name")

        remote = d.pop("remote")

        transport = d.pop("transport")

        def _parse_url(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        url = _parse_url(d.pop("url", UNSET))

        brought_server_preview = cls(
            already_present=already_present,
            command=command,
            name=name,
            remote=remote,
            transport=transport,
            url=url,
        )

        brought_server_preview.additional_properties = d
        return brought_server_preview

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
