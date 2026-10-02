from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.workflow_run_view_inputs import WorkflowRunViewInputs
    from ..models.workflow_run_view_output_type_0 import WorkflowRunViewOutputType0


T = TypeVar("T", bound="WorkflowRunView")


@_attrs_define
class WorkflowRunView:
    """A workflow run: what it was launched with, its status and its result.

    Attributes:
        depth (int): 1 for a run started on its own, one more per nested call.
        inputs (WorkflowRunViewInputs):
        run_id (str):
        started_at (str):
        status (str): `running`, `completed`, `failed` or `canceled`.
        workflow_id (str): `<package>/<workflow>`.
        ended_at (None | str | Unset):
        error (None | str | Unset): Why it failed or stopped.
        output (None | Unset | WorkflowRunViewOutputType0): What the workflow returned.
        parent_run_id (None | str | Unset): The run whose step started this one, when one did.
    """

    depth: int
    inputs: WorkflowRunViewInputs
    run_id: str
    started_at: str
    status: str
    workflow_id: str
    ended_at: None | str | Unset = UNSET
    error: None | str | Unset = UNSET
    output: None | Unset | WorkflowRunViewOutputType0 = UNSET
    parent_run_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.workflow_run_view_output_type_0 import WorkflowRunViewOutputType0

        depth = self.depth

        inputs = self.inputs.to_dict()

        run_id = self.run_id

        started_at = self.started_at

        status = self.status

        workflow_id = self.workflow_id

        ended_at: None | str | Unset
        if isinstance(self.ended_at, Unset):
            ended_at = UNSET
        else:
            ended_at = self.ended_at

        error: None | str | Unset
        if isinstance(self.error, Unset):
            error = UNSET
        else:
            error = self.error

        output: dict[str, Any] | None | Unset
        if isinstance(self.output, Unset):
            output = UNSET
        elif isinstance(self.output, WorkflowRunViewOutputType0):
            output = self.output.to_dict()
        else:
            output = self.output

        parent_run_id: None | str | Unset
        if isinstance(self.parent_run_id, Unset):
            parent_run_id = UNSET
        else:
            parent_run_id = self.parent_run_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "depth": depth,
                "inputs": inputs,
                "runId": run_id,
                "startedAt": started_at,
                "status": status,
                "workflowId": workflow_id,
            }
        )
        if ended_at is not UNSET:
            field_dict["endedAt"] = ended_at
        if error is not UNSET:
            field_dict["error"] = error
        if output is not UNSET:
            field_dict["output"] = output
        if parent_run_id is not UNSET:
            field_dict["parentRunId"] = parent_run_id

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.workflow_run_view_inputs import WorkflowRunViewInputs
        from ..models.workflow_run_view_output_type_0 import WorkflowRunViewOutputType0

        d = dict(src_dict)
        depth = d.pop("depth")

        inputs = WorkflowRunViewInputs.from_dict(d.pop("inputs"))

        run_id = d.pop("runId")

        started_at = d.pop("startedAt")

        status = d.pop("status")

        workflow_id = d.pop("workflowId")

        def _parse_ended_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        ended_at = _parse_ended_at(d.pop("endedAt", UNSET))

        def _parse_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error = _parse_error(d.pop("error", UNSET))

        def _parse_output(data: object) -> None | Unset | WorkflowRunViewOutputType0:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                output_type_0 = WorkflowRunViewOutputType0.from_dict(data)

                return output_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | Unset | WorkflowRunViewOutputType0, data)

        output = _parse_output(d.pop("output", UNSET))

        def _parse_parent_run_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        parent_run_id = _parse_parent_run_id(d.pop("parentRunId", UNSET))

        workflow_run_view = cls(
            depth=depth,
            inputs=inputs,
            run_id=run_id,
            started_at=started_at,
            status=status,
            workflow_id=workflow_id,
            ended_at=ended_at,
            error=error,
            output=output,
            parent_run_id=parent_run_id,
        )

        workflow_run_view.additional_properties = d
        return workflow_run_view

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
