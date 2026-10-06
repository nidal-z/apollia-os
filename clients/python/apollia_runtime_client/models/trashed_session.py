from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="TrashedSession")


@_attrs_define
class TrashedSession:
    """A conversation in the trash.

    Attributes:
        created_at (str): ISO-8601 creation timestamp.
        deleted_at (str): ISO-8601 timestamp of the move to the trash.
        id (str): Session identifier.
        mode (str): Mode string (`"libre"`, `"agent"` or `"companion"`).
        agent_name (None | str | Unset): Agent of the conversation, when it has one.
        title (None | str | Unset): Title the user or the model gave it, when it has one.
    """

    created_at: str
    deleted_at: str
    id: str
    mode: str
    agent_name: None | str | Unset = UNSET
    title: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at

        deleted_at = self.deleted_at

        id = self.id

        mode = self.mode

        agent_name: None | str | Unset
        if isinstance(self.agent_name, Unset):
            agent_name = UNSET
        else:
            agent_name = self.agent_name

        title: None | str | Unset
        if isinstance(self.title, Unset):
            title = UNSET
        else:
            title = self.title

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "created_at": created_at,
                "deleted_at": deleted_at,
                "id": id,
                "mode": mode,
            }
        )
        if agent_name is not UNSET:
            field_dict["agent_name"] = agent_name
        if title is not UNSET:
            field_dict["title"] = title

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = d.pop("created_at")

        deleted_at = d.pop("deleted_at")

        id = d.pop("id")

        mode = d.pop("mode")

        def _parse_agent_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent_name = _parse_agent_name(d.pop("agent_name", UNSET))

        def _parse_title(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        title = _parse_title(d.pop("title", UNSET))

        trashed_session = cls(
            created_at=created_at,
            deleted_at=deleted_at,
            id=id,
            mode=mode,
            agent_name=agent_name,
            title=title,
        )

        trashed_session.additional_properties = d
        return trashed_session

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
