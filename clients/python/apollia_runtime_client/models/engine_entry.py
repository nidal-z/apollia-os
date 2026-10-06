from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_name import EngineName
from ..models.engine_source import EngineSource
from ..models.engine_variant import EngineVariant
from ..types import UNSET, Unset

T = TypeVar("T", bound="EngineEntry")


@_attrs_define
class EngineEntry:
    """One engine on this machine.

    Attributes:
        engine (EngineName): An inference engine the store can hold.
        in_service (bool): True for the build this runtime starts for the model format it serves:
            the `llama-server` binary it resolves, the MLX environment it imports.
        path (str): Where it lives.
        source (EngineSource): How an engine reached this machine.
        tag (None | str | Unset): Upstream tag; unknown for an embedded engine.
        variant (EngineVariant | None | Unset):
        version (None | str | Unset): The version the engine states at its source: the `--version` banner of
            a binary, the package metadata of a Python environment. Unknown when it
            does not answer.
    """

    engine: EngineName
    in_service: bool
    path: str
    source: EngineSource
    tag: None | str | Unset = UNSET
    variant: EngineVariant | None | Unset = UNSET
    version: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        engine = self.engine.value

        in_service = self.in_service

        path = self.path

        source = self.source.value

        tag: None | str | Unset
        if isinstance(self.tag, Unset):
            tag = UNSET
        else:
            tag = self.tag

        variant: None | str | Unset
        if isinstance(self.variant, Unset):
            variant = UNSET
        elif isinstance(self.variant, EngineVariant):
            variant = self.variant.value
        else:
            variant = self.variant

        version: None | str | Unset
        if isinstance(self.version, Unset):
            version = UNSET
        else:
            version = self.version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "engine": engine,
                "in_service": in_service,
                "path": path,
                "source": source,
            }
        )
        if tag is not UNSET:
            field_dict["tag"] = tag
        if variant is not UNSET:
            field_dict["variant"] = variant
        if version is not UNSET:
            field_dict["version"] = version

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        engine = EngineName(d.pop("engine"))

        in_service = d.pop("in_service")

        path = d.pop("path")

        source = EngineSource(d.pop("source"))

        def _parse_tag(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        tag = _parse_tag(d.pop("tag", UNSET))

        def _parse_variant(data: object) -> EngineVariant | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                variant_type_1 = EngineVariant(data)

                return variant_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(EngineVariant | None | Unset, data)

        variant = _parse_variant(d.pop("variant", UNSET))

        def _parse_version(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        version = _parse_version(d.pop("version", UNSET))

        engine_entry = cls(
            engine=engine,
            in_service=in_service,
            path=path,
            source=source,
            tag=tag,
            variant=variant,
            version=version,
        )

        engine_entry.additional_properties = d
        return engine_entry

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
