from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.pending_approval_response_context_type_0 import PendingApprovalResponseContextType0





T = TypeVar("T", bound="PendingApprovalResponse")



@_attrs_define
class PendingApprovalResponse:
    """ One pending HITL approval entry.

        Attributes:
            agent_name (str):
            prompt (str):
            suspended_at (str):
            task_id (str):
            context (None | PendingApprovalResponseContextType0 | Unset):
     """

    agent_name: str
    prompt: str
    suspended_at: str
    task_id: str
    context: None | PendingApprovalResponseContextType0 | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.pending_approval_response_context_type_0 import PendingApprovalResponseContextType0
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


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agent_name": agent_name,
            "prompt": prompt,
            "suspended_at": suspended_at,
            "task_id": task_id,
        })
        if context is not UNSET:
            field_dict["context"] = context

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.pending_approval_response_context_type_0 import PendingApprovalResponseContextType0
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


        pending_approval_response = cls(
            agent_name=agent_name,
            prompt=prompt,
            suspended_at=suspended_at,
            task_id=task_id,
            context=context,
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
