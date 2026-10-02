from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.workflow_run_view import WorkflowRunView
    from ..models.workflow_step_view import WorkflowStepView


T = TypeVar("T", bound="WorkflowRunDetailView")


@_attrs_define
class WorkflowRunDetailView:
    """A run with its steps.

    Attributes:
        run (WorkflowRunView): A workflow run: what it was launched with, its status and its result.
        steps (list[WorkflowStepView]):
    """

    run: WorkflowRunView
    steps: list[WorkflowStepView]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        run = self.run.to_dict()

        steps = []
        for steps_item_data in self.steps:
            steps_item = steps_item_data.to_dict()
            steps.append(steps_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "run": run,
                "steps": steps,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.workflow_run_view import WorkflowRunView
        from ..models.workflow_step_view import WorkflowStepView

        d = dict(src_dict)
        run = WorkflowRunView.from_dict(d.pop("run"))

        steps = []
        _steps = d.pop("steps")
        for steps_item_data in _steps:
            steps_item = WorkflowStepView.from_dict(steps_item_data)

            steps.append(steps_item)

        workflow_run_detail_view = cls(
            run=run,
            steps=steps,
        )

        workflow_run_detail_view.additional_properties = d
        return workflow_run_detail_view

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
