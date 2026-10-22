from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.check_connection_body_probe_type_0 import CheckConnectionBodyProbeType0


T = TypeVar("T", bound="CheckConnectionBody")


@_attrs_define
class CheckConnectionBody:
    """Body of `POST /api/v1/connections/{id}/check`, which may be left out.

    Attributes:
        probe (CheckConnectionBodyProbeType0 | None | Unset): Read-only tool call an MCP server is probed with after its
            handshake,
            when its connector declares one; ignored for any other kind.
    """

    probe: CheckConnectionBodyProbeType0 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.check_connection_body_probe_type_0 import CheckConnectionBodyProbeType0

        probe: dict[str, Any] | None | Unset
        if isinstance(self.probe, Unset):
            probe = UNSET
        elif isinstance(self.probe, CheckConnectionBodyProbeType0):
            probe = self.probe.to_dict()
        else:
            probe = self.probe

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if probe is not UNSET:
            field_dict["probe"] = probe

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.check_connection_body_probe_type_0 import CheckConnectionBodyProbeType0

        d = dict(src_dict)

        def _parse_probe(data: object) -> CheckConnectionBodyProbeType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                probe_type_0 = CheckConnectionBodyProbeType0.from_dict(data)

                return probe_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(CheckConnectionBodyProbeType0 | None | Unset, data)

        probe = _parse_probe(d.pop("probe", UNSET))

        check_connection_body = cls(
            probe=probe,
        )

        check_connection_body.additional_properties = d
        return check_connection_body

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
