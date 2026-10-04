from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.journal_nature import JournalNature
from ..models.journal_outcome import JournalOutcome
from ..types import UNSET, Unset

T = TypeVar("T", bound="JournalSummary")


@_attrs_define
class JournalSummary:
    """One journal entry, said in a line and without content.

    Attributes:
        nature (JournalNature): What kind of fact an entry records.
        ts (str): When, RFC 3339 UTC.
        what (str): What happened, in a few words.
        agent (None | str | Unset): Agent the entry is about, when it is about one.
        family (None | str | Unset): Action family concerned (`read`, `write_or_delete`, ...), when there is one.
        outcome (JournalOutcome | None | Unset):
        target (None | str | Unset): What was acted on: a path, a recipient, the host of an address, a
            connection. Never an argument that carries content.
        tool (None | str | Unset): Tool concerned, when there is one.
    """

    nature: JournalNature
    ts: str
    what: str
    agent: None | str | Unset = UNSET
    family: None | str | Unset = UNSET
    outcome: JournalOutcome | None | Unset = UNSET
    target: None | str | Unset = UNSET
    tool: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        nature = self.nature.value

        ts = self.ts

        what = self.what

        agent: None | str | Unset
        if isinstance(self.agent, Unset):
            agent = UNSET
        else:
            agent = self.agent

        family: None | str | Unset
        if isinstance(self.family, Unset):
            family = UNSET
        else:
            family = self.family

        outcome: None | str | Unset
        if isinstance(self.outcome, Unset):
            outcome = UNSET
        elif isinstance(self.outcome, JournalOutcome):
            outcome = self.outcome.value
        else:
            outcome = self.outcome

        target: None | str | Unset
        if isinstance(self.target, Unset):
            target = UNSET
        else:
            target = self.target

        tool: None | str | Unset
        if isinstance(self.tool, Unset):
            tool = UNSET
        else:
            tool = self.tool

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "nature": nature,
                "ts": ts,
                "what": what,
            }
        )
        if agent is not UNSET:
            field_dict["agent"] = agent
        if family is not UNSET:
            field_dict["family"] = family
        if outcome is not UNSET:
            field_dict["outcome"] = outcome
        if target is not UNSET:
            field_dict["target"] = target
        if tool is not UNSET:
            field_dict["tool"] = tool

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        nature = JournalNature(d.pop("nature"))

        ts = d.pop("ts")

        what = d.pop("what")

        def _parse_agent(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent = _parse_agent(d.pop("agent", UNSET))

        def _parse_family(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        family = _parse_family(d.pop("family", UNSET))

        def _parse_outcome(data: object) -> JournalOutcome | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                outcome_type_1 = JournalOutcome(data)

                return outcome_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(JournalOutcome | None | Unset, data)

        outcome = _parse_outcome(d.pop("outcome", UNSET))

        def _parse_target(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        target = _parse_target(d.pop("target", UNSET))

        def _parse_tool(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        tool = _parse_tool(d.pop("tool", UNSET))

        journal_summary = cls(
            nature=nature,
            ts=ts,
            what=what,
            agent=agent,
            family=family,
            outcome=outcome,
            target=target,
            tool=tool,
        )

        journal_summary.additional_properties = d
        return journal_summary

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
