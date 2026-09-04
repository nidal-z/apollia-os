from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.broken_link import BrokenLink





T = TypeVar("T", bound="VerifyChainReport")



@_attrs_define
class VerifyChainReport:
    """ Outcome of verifying a run's chain.

        Attributes:
            entries_checked (int): Number of entries inspected (all of them when `ok`).
            ok (bool): `true` when the whole chain verified.
            run_id (str): Run that was verified.
            first_broken_link (BrokenLink | None | Unset):
     """

    entries_checked: int
    ok: bool
    run_id: str
    first_broken_link: BrokenLink | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.broken_link import BrokenLink
        entries_checked = self.entries_checked

        ok = self.ok

        run_id = self.run_id

        first_broken_link: dict[str, Any] | None | Unset
        if isinstance(self.first_broken_link, Unset):
            first_broken_link = UNSET
        elif isinstance(self.first_broken_link, BrokenLink):
            first_broken_link = self.first_broken_link.to_dict()
        else:
            first_broken_link = self.first_broken_link


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "entries_checked": entries_checked,
            "ok": ok,
            "run_id": run_id,
        })
        if first_broken_link is not UNSET:
            field_dict["first_broken_link"] = first_broken_link

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.broken_link import BrokenLink
        d = dict(src_dict)
        entries_checked = d.pop("entries_checked")

        ok = d.pop("ok")

        run_id = d.pop("run_id")

        def _parse_first_broken_link(data: object) -> BrokenLink | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                first_broken_link_type_1 = BrokenLink.from_dict(data)



                return first_broken_link_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(BrokenLink | None | Unset, data)

        first_broken_link = _parse_first_broken_link(d.pop("first_broken_link", UNSET))


        verify_chain_report = cls(
            entries_checked=entries_checked,
            ok=ok,
            run_id=run_id,
            first_broken_link=first_broken_link,
        )


        verify_chain_report.additional_properties = d
        return verify_chain_report

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
