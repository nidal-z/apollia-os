from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="ResolvedApprovalResponse")



@_attrs_define
class ResolvedApprovalResponse:
    """ One resolved HITL approval entry.

        Attributes:
            agent_name (str):
            approved (bool):
            task_id (str):
            reason (None | str | Unset):
            responded_at (None | str | Unset):
            wait_duration_ms (int | None | Unset):
     """

    agent_name: str
    approved: bool
    task_id: str
    reason: None | str | Unset = UNSET
    responded_at: None | str | Unset = UNSET
    wait_duration_ms: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        agent_name = self.agent_name

        approved = self.approved

        task_id = self.task_id

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason

        responded_at: None | str | Unset
        if isinstance(self.responded_at, Unset):
            responded_at = UNSET
        else:
            responded_at = self.responded_at

        wait_duration_ms: int | None | Unset
        if isinstance(self.wait_duration_ms, Unset):
            wait_duration_ms = UNSET
        else:
            wait_duration_ms = self.wait_duration_ms


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agent_name": agent_name,
            "approved": approved,
            "task_id": task_id,
        })
        if reason is not UNSET:
            field_dict["reason"] = reason
        if responded_at is not UNSET:
            field_dict["responded_at"] = responded_at
        if wait_duration_ms is not UNSET:
            field_dict["wait_duration_ms"] = wait_duration_ms

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent_name = d.pop("agent_name")

        approved = d.pop("approved")

        task_id = d.pop("task_id")

        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))


        def _parse_responded_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        responded_at = _parse_responded_at(d.pop("responded_at", UNSET))


        def _parse_wait_duration_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        wait_duration_ms = _parse_wait_duration_ms(d.pop("wait_duration_ms", UNSET))


        resolved_approval_response = cls(
            agent_name=agent_name,
            approved=approved,
            task_id=task_id,
            reason=reason,
            responded_at=responded_at,
            wait_duration_ms=wait_duration_ms,
        )


        resolved_approval_response.additional_properties = d
        return resolved_approval_response

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
