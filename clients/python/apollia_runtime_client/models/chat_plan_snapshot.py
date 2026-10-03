from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.chat_plan_snapshot_plan_type_0 import ChatPlanSnapshotPlanType0


T = TypeVar("T", bound="ChatPlanSnapshot")


@_attrs_define
class ChatPlanSnapshot:
    """Snapshot of a session's plan plus its current plan-mode phase.

    The phase is the authoritative gate state: a persisted plan keeps its
    `awaiting_approval` status forever, but once approved the session moves to
    `executing` then `done`. The desktop hydrates the plan tab from `plan` and
    decides whether to show the approval card from `phase`, never from the
    plan's own status.

        Attributes:
            phase (str): Session plan-mode phase: `discovery`, `drafting`, `awaiting_approval`,
                `executing`, or `done`.
            plan (ChatPlanSnapshotPlanType0 | None | Unset): Current plan, or `None` when the session has produced none.
    """

    phase: str
    plan: ChatPlanSnapshotPlanType0 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.chat_plan_snapshot_plan_type_0 import ChatPlanSnapshotPlanType0

        phase = self.phase

        plan: dict[str, Any] | None | Unset
        if isinstance(self.plan, Unset):
            plan = UNSET
        elif isinstance(self.plan, ChatPlanSnapshotPlanType0):
            plan = self.plan.to_dict()
        else:
            plan = self.plan

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "phase": phase,
            }
        )
        if plan is not UNSET:
            field_dict["plan"] = plan

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.chat_plan_snapshot_plan_type_0 import ChatPlanSnapshotPlanType0

        d = dict(src_dict)
        phase = d.pop("phase")

        def _parse_plan(data: object) -> ChatPlanSnapshotPlanType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                plan_type_0 = ChatPlanSnapshotPlanType0.from_dict(data)

                return plan_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(ChatPlanSnapshotPlanType0 | None | Unset, data)

        plan = _parse_plan(d.pop("plan", UNSET))

        chat_plan_snapshot = cls(
            phase=phase,
            plan=plan,
        )

        chat_plan_snapshot.additional_properties = d
        return chat_plan_snapshot

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
