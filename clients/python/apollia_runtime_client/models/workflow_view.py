from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.input_field import InputField


T = TypeVar("T", bound="WorkflowView")


@_attrs_define
class WorkflowView:
    """A workflow as its package declares it, with the fields of its inputs.

    Attributes:
        description (str):
        inputs (list[InputField]): One field per input, in the order the workflow declares them, each
            with the kind of field a launch form shows for it.
        steps (list[str]): The declared steps, in order.
        version (str): The version of the package that declares it.
        workflow_id (str): `<package>/<workflow>`.
    """

    description: str
    inputs: list[InputField]
    steps: list[str]
    version: str
    workflow_id: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        description = self.description

        inputs = []
        for inputs_item_data in self.inputs:
            inputs_item = inputs_item_data.to_dict()
            inputs.append(inputs_item)

        steps = self.steps

        version = self.version

        workflow_id = self.workflow_id

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "description": description,
                "inputs": inputs,
                "steps": steps,
                "version": version,
                "workflowId": workflow_id,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.input_field import InputField

        d = dict(src_dict)
        description = d.pop("description")

        inputs = []
        _inputs = d.pop("inputs")
        for inputs_item_data in _inputs:
            inputs_item = InputField.from_dict(inputs_item_data)

            inputs.append(inputs_item)

        steps = cast(list[str], d.pop("steps"))

        version = d.pop("version")

        workflow_id = d.pop("workflowId")

        workflow_view = cls(
            description=description,
            inputs=inputs,
            steps=steps,
            version=version,
            workflow_id=workflow_id,
        )

        workflow_view.additional_properties = d
        return workflow_view

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
