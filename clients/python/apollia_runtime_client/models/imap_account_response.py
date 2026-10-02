from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ImapAccountResponse")


@_attrs_define
class ImapAccountResponse:
    """A verified mailbox, as both routes answer it.

    Attributes:
        connection_id (str): Identifier of the connection, `imap:<address>`.
        email (str): Address of the mailbox.
        host (str): IMAP server.
        port (int): IMAP port.
        username (str): Identifier sent at login.
    """

    connection_id: str
    email: str
    host: str
    port: int
    username: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        connection_id = self.connection_id

        email = self.email

        host = self.host

        port = self.port

        username = self.username

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "connectionId": connection_id,
                "email": email,
                "host": host,
                "port": port,
                "username": username,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        connection_id = d.pop("connectionId")

        email = d.pop("email")

        host = d.pop("host")

        port = d.pop("port")

        username = d.pop("username")

        imap_account_response = cls(
            connection_id=connection_id,
            email=email,
            host=host,
            port=port,
            username=username,
        )

        imap_account_response.additional_properties = d
        return imap_account_response

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
