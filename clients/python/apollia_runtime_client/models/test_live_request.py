from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.test_live_request_probe_type_0 import TestLiveRequestProbeType0





T = TypeVar("T", bound="TestLiveRequest")



@_attrs_define
class TestLiveRequest:
    """ Body for [`test_live_server`]: the optional read-only probe to run against
    the live session, supplied by the desktop from the connector enrichment.

        Attributes:
            probe (None | TestLiveRequestProbeType0 | Unset): Read-only probe declared for this connector, when any.
     """

    probe: None | TestLiveRequestProbeType0 | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.test_live_request_probe_type_0 import TestLiveRequestProbeType0
        probe: dict[str, Any] | None | Unset
        if isinstance(self.probe, Unset):
            probe = UNSET
        elif isinstance(self.probe, TestLiveRequestProbeType0):
            probe = self.probe.to_dict()
        else:
            probe = self.probe


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
        })
        if probe is not UNSET:
            field_dict["probe"] = probe

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.test_live_request_probe_type_0 import TestLiveRequestProbeType0
        d = dict(src_dict)
        def _parse_probe(data: object) -> None | TestLiveRequestProbeType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                probe_type_0 = TestLiveRequestProbeType0.from_dict(data)



                return probe_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | TestLiveRequestProbeType0 | Unset, data)

        probe = _parse_probe(d.pop("probe", UNSET))


        test_live_request = cls(
            probe=probe,
        )


        test_live_request.additional_properties = d
        return test_live_request

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
