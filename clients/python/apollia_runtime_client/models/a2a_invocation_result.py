from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.a2a_invocation_result_result import A2AInvocationResultResult


T = TypeVar("T", bound="A2AInvocationResult")


@_attrs_define
class A2AInvocationResult:
    """Result of a successful A2A invocation.

    Attributes:
        agent_name (str): Name of the Worker Agent that handled the invocation.
        duration_ms (int): Total invocation duration in milliseconds.
        result (A2AInvocationResultResult): AIP result returned by the Worker Agent.
        skill_id (str): Identifier of the invoked skill.
        run_id (None | str | Unset): The run the invoked task journals under, the key of
            `GET /api/v1/audit/journal/{run_id}`.

            Not the same identifier as `result.task_id`: both are minted at
            submission and the journal is stored under this one. `null` only when
            the invocation did not go through the task router.
    """

    agent_name: str
    duration_ms: int
    result: A2AInvocationResultResult
    skill_id: str
    run_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent_name = self.agent_name

        duration_ms = self.duration_ms

        result = self.result.to_dict()

        skill_id = self.skill_id

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
                "duration_ms": duration_ms,
                "result": result,
                "skill_id": skill_id,
            }
        )
        if run_id is not UNSET:
            field_dict["run_id"] = run_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.a2a_invocation_result_result import A2AInvocationResultResult

        d = dict(src_dict)
        agent_name = d.pop("agent_name")

        duration_ms = d.pop("duration_ms")

        result = A2AInvocationResultResult.from_dict(d.pop("result"))

        skill_id = d.pop("skill_id")

        def _parse_run_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        run_id = _parse_run_id(d.pop("run_id", UNSET))

        a2a_invocation_result = cls(
            agent_name=agent_name,
            duration_ms=duration_ms,
            result=result,
            skill_id=skill_id,
            run_id=run_id,
        )

        a2a_invocation_result.additional_properties = d
        return a2a_invocation_result

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
