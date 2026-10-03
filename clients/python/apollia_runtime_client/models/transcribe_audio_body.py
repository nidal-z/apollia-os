from __future__ import annotations

from collections.abc import Mapping
from io import BytesIO
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from .. import types
from ..models.transcribe_audio_body_source import TranscribeAudioBodySource
from ..types import UNSET, File, Unset

T = TypeVar("T", bound="TranscribeAudioBody")


@_attrs_define
class TranscribeAudioBody:
    """
    Attributes:
        audio (File): WAV audio file
        language (str | Unset): Language hint, an ISO 639-1 code
        source (TranscribeAudioBodySource | Unset): What the audio is, the tag the history shows: api (default), file or
            hotkey
    """

    audio: File
    language: str | Unset = UNSET
    source: TranscribeAudioBodySource | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        audio = self.audio.to_tuple()

        language = self.language

        source: str | Unset = UNSET
        if not isinstance(self.source, Unset):
            source = self.source.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "audio": audio,
            }
        )
        if language is not UNSET:
            field_dict["language"] = language
        if source is not UNSET:
            field_dict["source"] = source

        return field_dict

    def to_multipart(self) -> types.RequestFiles:
        files: types.RequestFiles = []

        files.append(("audio", self.audio.to_tuple()))

        if not isinstance(self.language, Unset):
            files.append(("language", (None, str(self.language).encode(), "text/plain")))

        if not isinstance(self.source, Unset):
            files.append(("source", (None, str(self.source.value).encode(), "text/plain")))

        for prop_name, prop in self.additional_properties.items():
            files.append((prop_name, (None, str(prop).encode(), "text/plain")))

        return files

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        audio = File(payload=BytesIO(d.pop("audio")))

        language = d.pop("language", UNSET)

        _source = d.pop("source", UNSET)
        source: TranscribeAudioBodySource | Unset
        if isinstance(_source, Unset):
            source = UNSET
        else:
            source = TranscribeAudioBodySource(_source)

        transcribe_audio_body = cls(
            audio=audio,
            language=language,
            source=source,
        )

        transcribe_audio_body.additional_properties = d
        return transcribe_audio_body

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
