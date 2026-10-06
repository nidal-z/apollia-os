from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define

T = TypeVar("T", bound="SessionNotes")


@_attrs_define
class SessionNotes:
    """The notes of a conversation, as `GET` answers them and `PUT` takes them.

    Attributes:
        notes (str): Free text the user keeps beside the conversation. It is never sent to
            a model.
    """

    notes: str

    def to_dict(self) -> dict[str, Any]:
        notes = self.notes

        field_dict: dict[str, Any] = {}

        field_dict.update(
            {
                "notes": notes,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        notes = d.pop("notes")

        session_notes = cls(
            notes=notes,
        )

        return session_notes
