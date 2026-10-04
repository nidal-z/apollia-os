from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="Artifact")


@_attrs_define
class Artifact:
    """One artifact, as the chat draws it.

    Attributes:
        content (str): Full textual content.
        created_at (str): ISO 8601 creation timestamp.
        id (str): UUID v4 identifier.
        kind (str): Kind bucket: `"code"`, `"file"`, `"spec"`, `"bash_output"`, `"other"`.
        session_id (str): Owning chat session identifier.
        title (str): Human-readable title (path, heading, short description).
        updated_at (str): ISO 8601 last-update timestamp.
        language (None | str | Unset): Detected or declared language (e.g. `"rust"`, `"markdown"`).
        source_message_id (None | str | Unset): Source message identifier (absent for a manual creation).
        source_tool (None | str | Unset): Source tool name (e.g. `"file_write"`, `"bash_executor"`).
    """

    content: str
    created_at: str
    id: str
    kind: str
    session_id: str
    title: str
    updated_at: str
    language: None | str | Unset = UNSET
    source_message_id: None | str | Unset = UNSET
    source_tool: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        content = self.content

        created_at = self.created_at

        id = self.id

        kind = self.kind

        session_id = self.session_id

        title = self.title

        updated_at = self.updated_at

        language: None | str | Unset
        if isinstance(self.language, Unset):
            language = UNSET
        else:
            language = self.language

        source_message_id: None | str | Unset
        if isinstance(self.source_message_id, Unset):
            source_message_id = UNSET
        else:
            source_message_id = self.source_message_id

        source_tool: None | str | Unset
        if isinstance(self.source_tool, Unset):
            source_tool = UNSET
        else:
            source_tool = self.source_tool

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "content": content,
                "created_at": created_at,
                "id": id,
                "kind": kind,
                "session_id": session_id,
                "title": title,
                "updated_at": updated_at,
            }
        )
        if language is not UNSET:
            field_dict["language"] = language
        if source_message_id is not UNSET:
            field_dict["source_message_id"] = source_message_id
        if source_tool is not UNSET:
            field_dict["source_tool"] = source_tool

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        content = d.pop("content")

        created_at = d.pop("created_at")

        id = d.pop("id")

        kind = d.pop("kind")

        session_id = d.pop("session_id")

        title = d.pop("title")

        updated_at = d.pop("updated_at")

        def _parse_language(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        language = _parse_language(d.pop("language", UNSET))

        def _parse_source_message_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        source_message_id = _parse_source_message_id(d.pop("source_message_id", UNSET))

        def _parse_source_tool(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        source_tool = _parse_source_tool(d.pop("source_tool", UNSET))

        artifact = cls(
            content=content,
            created_at=created_at,
            id=id,
            kind=kind,
            session_id=session_id,
            title=title,
            updated_at=updated_at,
            language=language,
            source_message_id=source_message_id,
            source_tool=source_tool,
        )

        artifact.additional_properties = d
        return artifact

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
