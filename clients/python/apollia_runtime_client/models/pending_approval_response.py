from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.pending_approval_response_context_type_0 import (
        PendingApprovalResponseContextType0,
    )
    from ..models.pending_approval_response_payload_type_0 import (
        PendingApprovalResponsePayloadType0,
    )


T = TypeVar("T", bound="PendingApprovalResponse")


@_attrs_define
class PendingApprovalResponse:
    """One pending HITL approval entry.

    Carries the same pause as a `GET /api/v1/tasks?status=input_required` item,
    typed payload included, so a card can be drawn from either list.

        Attributes:
            agent_name (str):
            prompt (str):
            suspended_at (str):
            task_id (str):
            context (None | PendingApprovalResponseContextType0 | Unset):
            payload (None | PendingApprovalResponsePayloadType0 | Unset): Typed question or approval of the pause, `null`
                for a prompt-only pause.
                The body of `POST /api/v1/tasks/{id}/resume` answers it.
            skill_id (None | str | Unset): Skill that paused, when the pause came from a skill.
    """

    agent_name: str
    prompt: str
    suspended_at: str
    task_id: str
    context: None | PendingApprovalResponseContextType0 | Unset = UNSET
    payload: None | PendingApprovalResponsePayloadType0 | Unset = UNSET
    skill_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.pending_approval_response_context_type_0 import (
            PendingApprovalResponseContextType0,
        )
        from ..models.pending_approval_response_payload_type_0 import (
            PendingApprovalResponsePayloadType0,
        )

        agent_name = self.agent_name

        prompt = self.prompt

        suspended_at = self.suspended_at

        task_id = self.task_id

        context: dict[str, Any] | None | Unset
        if isinstance(self.context, Unset):
            context = UNSET
        elif isinstance(self.context, PendingApprovalResponseContextType0):
            context = self.context.to_dict()
        else:
            context = self.context

        payload: dict[str, Any] | None | Unset
        if isinstance(self.payload, Unset):
            payload = UNSET
        elif isinstance(self.payload, PendingApprovalResponsePayloadType0):
            payload = self.payload.to_dict()
        else:
            payload = self.payload

        skill_id: None | str | Unset
        if isinstance(self.skill_id, Unset):
            skill_id = UNSET
        else:
            skill_id = self.skill_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent_name": agent_name,
                "prompt": prompt,
                "suspended_at": suspended_at,
                "task_id": task_id,
            }
        )
        if context is not UNSET:
            field_dict["context"] = context
        if payload is not UNSET:
            field_dict["payload"] = payload
        if skill_id is not UNSET:
            field_dict["skill_id"] = skill_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.pending_approval_response_context_type_0 import (
            PendingApprovalResponseContextType0,
        )
        from ..models.pending_approval_response_payload_type_0 import (
            PendingApprovalResponsePayloadType0,
        )

        d = dict(src_dict)
        agent_name = d.pop("agent_name")

        prompt = d.pop("prompt")

        suspended_at = d.pop("suspended_at")

        task_id = d.pop("task_id")

        def _parse_context(data: object) -> None | PendingApprovalResponseContextType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                context_type_0 = PendingApprovalResponseContextType0.from_dict(data)

                return context_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | PendingApprovalResponseContextType0 | Unset, data)

        context = _parse_context(d.pop("context", UNSET))

        def _parse_payload(data: object) -> None | PendingApprovalResponsePayloadType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                payload_type_0 = PendingApprovalResponsePayloadType0.from_dict(data)

                return payload_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | PendingApprovalResponsePayloadType0 | Unset, data)

        payload = _parse_payload(d.pop("payload", UNSET))

        def _parse_skill_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        skill_id = _parse_skill_id(d.pop("skill_id", UNSET))

        pending_approval_response = cls(
            agent_name=agent_name,
            prompt=prompt,
            suspended_at=suspended_at,
            task_id=task_id,
            context=context,
            payload=payload,
            skill_id=skill_id,
        )

        pending_approval_response.additional_properties = d
        return pending_approval_response

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
