from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.journal_break import JournalBreak





T = TypeVar("T", bound="VerifyJournalReport")



@_attrs_define
class VerifyJournalReport:
    """ Outcome of verifying the whole journal across all runs.

        Attributes:
            entries_checked (int): Number of globally-chained entries inspected.
            head_matches_state (bool): Whether the terminal global head matches the persisted state anchor. A
                `false` here with no `first_break` signals global-tail truncation or a
                rolled-back state row (detectable in-database only until the key is
                compromised; export the anchor off-machine for a durable guarantee).
            ok (bool): `true` when the global chain, every per-run chain, and the head anchor
                all verified.
            runs_checked (int): Number of distinct runs covered by the global chain.
            first_break (JournalBreak | None | Unset):
     """

    entries_checked: int
    head_matches_state: bool
    ok: bool
    runs_checked: int
    first_break: JournalBreak | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.journal_break import JournalBreak
        entries_checked = self.entries_checked

        head_matches_state = self.head_matches_state

        ok = self.ok

        runs_checked = self.runs_checked

        first_break: dict[str, Any] | None | Unset
        if isinstance(self.first_break, Unset):
            first_break = UNSET
        elif isinstance(self.first_break, JournalBreak):
            first_break = self.first_break.to_dict()
        else:
            first_break = self.first_break


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "entries_checked": entries_checked,
            "head_matches_state": head_matches_state,
            "ok": ok,
            "runs_checked": runs_checked,
        })
        if first_break is not UNSET:
            field_dict["first_break"] = first_break

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.journal_break import JournalBreak
        d = dict(src_dict)
        entries_checked = d.pop("entries_checked")

        head_matches_state = d.pop("head_matches_state")

        ok = d.pop("ok")

        runs_checked = d.pop("runs_checked")

        def _parse_first_break(data: object) -> JournalBreak | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                first_break_type_1 = JournalBreak.from_dict(data)



                return first_break_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(JournalBreak | None | Unset, data)

        first_break = _parse_first_break(d.pop("first_break", UNSET))


        verify_journal_report = cls(
            entries_checked=entries_checked,
            head_matches_state=head_matches_state,
            ok=ok,
            runs_checked=runs_checked,
            first_break=first_break,
        )


        verify_journal_report.additional_properties = d
        return verify_journal_report

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
