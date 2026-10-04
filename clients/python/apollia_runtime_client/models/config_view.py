from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.config_diagnostic_body import ConfigDiagnosticBody
    from ..models.locked_key_body import LockedKeyBody


T = TypeVar("T", bound="ConfigView")


@_attrs_define
class ConfigView:
    """`apollia.toml` as the runtime reads it.

    Attributes:
        diagnostics (list[ConfigDiagnosticBody]): The faults of the text, each at its line; empty when it is valid.
        exists (bool): Whether the file exists; an absent file reads as an empty text.
        locked (list[LockedKeyBody]): The keys the organisation policy fixes, with their value.
        path (str): Where the file lives.
        pending_restart (list[str]): Keys changed in the file that wait for a restart of the runtime.
        sha256 (str): SHA-256 of `text`, to present when saving.
        text (str): The text of the file.
    """

    diagnostics: list[ConfigDiagnosticBody]
    exists: bool
    locked: list[LockedKeyBody]
    path: str
    pending_restart: list[str]
    sha256: str
    text: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        diagnostics = []
        for diagnostics_item_data in self.diagnostics:
            diagnostics_item = diagnostics_item_data.to_dict()
            diagnostics.append(diagnostics_item)

        exists = self.exists

        locked = []
        for locked_item_data in self.locked:
            locked_item = locked_item_data.to_dict()
            locked.append(locked_item)

        path = self.path

        pending_restart = self.pending_restart

        sha256 = self.sha256

        text = self.text

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "diagnostics": diagnostics,
                "exists": exists,
                "locked": locked,
                "path": path,
                "pendingRestart": pending_restart,
                "sha256": sha256,
                "text": text,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.config_diagnostic_body import ConfigDiagnosticBody
        from ..models.locked_key_body import LockedKeyBody

        d = dict(src_dict)
        diagnostics = []
        _diagnostics = d.pop("diagnostics")
        for diagnostics_item_data in _diagnostics:
            diagnostics_item = ConfigDiagnosticBody.from_dict(diagnostics_item_data)

            diagnostics.append(diagnostics_item)

        exists = d.pop("exists")

        locked = []
        _locked = d.pop("locked")
        for locked_item_data in _locked:
            locked_item = LockedKeyBody.from_dict(locked_item_data)

            locked.append(locked_item)

        path = d.pop("path")

        pending_restart = cast(list[str], d.pop("pendingRestart"))

        sha256 = d.pop("sha256")

        text = d.pop("text")

        config_view = cls(
            diagnostics=diagnostics,
            exists=exists,
            locked=locked,
            path=path,
            pending_restart=pending_restart,
            sha256=sha256,
            text=text,
        )

        config_view.additional_properties = d
        return config_view

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
