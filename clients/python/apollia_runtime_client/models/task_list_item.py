from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.task_list_item_payload_type_0 import TaskListItemPayloadType0


T = TypeVar("T", bound="TaskListItem")


@_attrs_define
class TaskListItem:
    """One entry in the task list.

    Attributes:
        agent_id (str): Agent that owns this task.
        status (str): Current task status.
        task_id (str): Unique task identifier.
        agent (None | str | Unset): Name of the agent that paused. Present only on an `input_required` task.
        created_at (None | str | Unset): ISO 8601 creation timestamp. Present only on an `input_required` task.
        error (None | str | Unset): Failure reason for a failed task (parity with `task status`); `null`
            otherwise. Kept unconditionally so the schema is stable for automation.
        error_code (None | str | Unset): Structured failure code parsed from the error (e.g. `BAD_MESSAGE`).
        payload (None | TaskListItemPayloadType0 | Unset): The typed question or approval the task is waiting on.
            Present only on
            an `input_required` task whose pause carries one.
        prompt (None | str | Unset): Sentence shown to the human. Present only on an `input_required` task.
        skill (None | str | Unset): Skill that paused. Present only on an `input_required` task that paused
            from a skill.
    """

    agent_id: str
    status: str
    task_id: str
    agent: None | str | Unset = UNSET
    created_at: None | str | Unset = UNSET
    error: None | str | Unset = UNSET
    error_code: None | str | Unset = UNSET
    payload: None | TaskListItemPayloadType0 | Unset = UNSET
    prompt: None | str | Unset = UNSET
    skill: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.task_list_item_payload_type_0 import TaskListItemPayloadType0

        agent_id = self.agent_id

        status = self.status

        task_id = self.task_id

        agent: None | str | Unset
        if isinstance(self.agent, Unset):
            agent = UNSET
        else:
            agent = self.agent

        created_at: None | str | Unset
        if isinstance(self.created_at, Unset):
            created_at = UNSET
        else:
            created_at = self.created_at

        error: None | str | Unset
        if isinstance(self.error, Unset):
            error = UNSET
        else:
            error = self.error

        error_code: None | str | Unset
        if isinstance(self.error_code, Unset):
            error_code = UNSET
        else:
            error_code = self.error_code

        payload: dict[str, Any] | None | Unset
        if isinstance(self.payload, Unset):
            payload = UNSET
        elif isinstance(self.payload, TaskListItemPayloadType0):
            payload = self.payload.to_dict()
        else:
            payload = self.payload

        prompt: None | str | Unset
        if isinstance(self.prompt, Unset):
            prompt = UNSET
        else:
            prompt = self.prompt

        skill: None | str | Unset
        if isinstance(self.skill, Unset):
            skill = UNSET
        else:
            skill = self.skill

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent_id": agent_id,
                "status": status,
                "task_id": task_id,
            }
        )
        if agent is not UNSET:
            field_dict["agent"] = agent
        if created_at is not UNSET:
            field_dict["created_at"] = created_at
        if error is not UNSET:
            field_dict["error"] = error
        if error_code is not UNSET:
            field_dict["error_code"] = error_code
        if payload is not UNSET:
            field_dict["payload"] = payload
        if prompt is not UNSET:
            field_dict["prompt"] = prompt
        if skill is not UNSET:
            field_dict["skill"] = skill

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.task_list_item_payload_type_0 import TaskListItemPayloadType0

        d = dict(src_dict)
        agent_id = d.pop("agent_id")

        status = d.pop("status")

        task_id = d.pop("task_id")

        def _parse_agent(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent = _parse_agent(d.pop("agent", UNSET))

        def _parse_created_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        created_at = _parse_created_at(d.pop("created_at", UNSET))

        def _parse_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error = _parse_error(d.pop("error", UNSET))

        def _parse_error_code(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error_code = _parse_error_code(d.pop("error_code", UNSET))

        def _parse_payload(data: object) -> None | TaskListItemPayloadType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                payload_type_0 = TaskListItemPayloadType0.from_dict(data)

                return payload_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | TaskListItemPayloadType0 | Unset, data)

        payload = _parse_payload(d.pop("payload", UNSET))

        def _parse_prompt(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        prompt = _parse_prompt(d.pop("prompt", UNSET))

        def _parse_skill(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        skill = _parse_skill(d.pop("skill", UNSET))

        task_list_item = cls(
            agent_id=agent_id,
            status=status,
            task_id=task_id,
            agent=agent,
            created_at=created_at,
            error=error,
            error_code=error_code,
            payload=payload,
            prompt=prompt,
            skill=skill,
        )

        task_list_item.additional_properties = d
        return task_list_item

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
