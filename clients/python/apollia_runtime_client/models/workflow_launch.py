from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.workflow_launch_inputs import WorkflowLaunchInputs


T = TypeVar("T", bound="WorkflowLaunch")


@_attrs_define
class WorkflowLaunch:
    """Body of `POST /api/v1/workflows/{package}/{workflow}/runs`.

    Attributes:
        inputs (WorkflowLaunchInputs | Unset): The inputs, checked against the schema the workflow declares. A text
            value given for a field of another type is read as that type when it
            parses as one.
    """

    inputs: WorkflowLaunchInputs | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        inputs: dict[str, Any] | Unset = UNSET
        if not isinstance(self.inputs, Unset):
            inputs = self.inputs.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if inputs is not UNSET:
            field_dict["inputs"] = inputs

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.workflow_launch_inputs import WorkflowLaunchInputs

        d = dict(src_dict)
        _inputs = d.pop("inputs", UNSET)
        inputs: WorkflowLaunchInputs | Unset
        if isinstance(_inputs, Unset):
            inputs = UNSET
        else:
            inputs = WorkflowLaunchInputs.from_dict(_inputs)

        workflow_launch = cls(
            inputs=inputs,
        )

        workflow_launch.additional_properties = d
        return workflow_launch

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
