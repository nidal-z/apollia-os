from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="A2ADelegateResult")


@_attrs_define
class A2ADelegateResult:
    """Result of a successful A2A delegation.

    Attributes:
        agent_name (str): Name of the Worker Agent that handled the delegation.
        output (str): Text output produced by the Worker Agent.
        task_id (str): Identifier of the task executed by the Worker Agent.
        run_id (None | str | Unset): The run that task journals under, when the delegation went through the
            task router.

            Distinct from `task_id`, and it is the key of
            `GET /api/v1/audit/journal/{run_id}`: a caller given only the task id
            looks the journal up under an identifier nothing is stored under.
            `None` for a delegate that does not submit through the router.
    """

    agent_name: str
    output: str
    task_id: str
    run_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent_name = self.agent_name

        output = self.output

        task_id = self.task_id

        run_id: None | str | Unset
        if isinstance(self.run_id, Unset):
            run_id = UNSET
        else:
            run_id = self.run_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent_name": agent_name,
                "output": output,
                "task_id": task_id,
            }
        )
        if run_id is not UNSET:
            field_dict["run_id"] = run_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent_name = d.pop("agent_name")

        output = d.pop("output")

        task_id = d.pop("task_id")

        def _parse_run_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        run_id = _parse_run_id(d.pop("run_id", UNSET))

        a2a_delegate_result = cls(
            agent_name=agent_name,
            output=output,
            task_id=task_id,
            run_id=run_id,
        )

        a2a_delegate_result.additional_properties = d
        return a2a_delegate_result

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
