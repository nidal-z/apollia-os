from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.trigger_source_input import TriggerSourceInput
    from ..models.update_trigger_request_workflow_inputs_type_0 import (
        UpdateTriggerRequestWorkflowInputsType0,
    )


T = TypeVar("T", bound="UpdateTriggerRequest")


@_attrs_define
class UpdateTriggerRequest:
    """Request body for `PUT /api/v1/triggers/:id`, trigger update.

    Attributes:
        source (TriggerSourceInput): Trigger source description used in create/update requests.
        agent (None | str | Unset): Target agent; exclusive with `workflow`.
        enabled (bool | None | Unset): Whether the trigger is active.
        input_template (None | str | Unset): The message of an agent target.
        on_busy (None | str | Unset): Policy when the agent is busy.
        workflow (None | str | Unset): Target workflow, `<package>/<workflow>`; exclusive with `agent`.
        workflow_inputs (None | Unset | UpdateTriggerRequestWorkflowInputsType0): For a workflow target, one text
            template per input field.
    """

    source: TriggerSourceInput
    agent: None | str | Unset = UNSET
    enabled: bool | None | Unset = UNSET
    input_template: None | str | Unset = UNSET
    on_busy: None | str | Unset = UNSET
    workflow: None | str | Unset = UNSET
    workflow_inputs: None | Unset | UpdateTriggerRequestWorkflowInputsType0 = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.update_trigger_request_workflow_inputs_type_0 import (
            UpdateTriggerRequestWorkflowInputsType0,
        )

        source = self.source.to_dict()

        agent: None | str | Unset
        if isinstance(self.agent, Unset):
            agent = UNSET
        else:
            agent = self.agent

        enabled: bool | None | Unset
        if isinstance(self.enabled, Unset):
            enabled = UNSET
        else:
            enabled = self.enabled

        input_template: None | str | Unset
        if isinstance(self.input_template, Unset):
            input_template = UNSET
        else:
            input_template = self.input_template

        on_busy: None | str | Unset
        if isinstance(self.on_busy, Unset):
            on_busy = UNSET
        else:
            on_busy = self.on_busy

        workflow: None | str | Unset
        if isinstance(self.workflow, Unset):
            workflow = UNSET
        else:
            workflow = self.workflow

        workflow_inputs: dict[str, Any] | None | Unset
        if isinstance(self.workflow_inputs, Unset):
            workflow_inputs = UNSET
        elif isinstance(self.workflow_inputs, UpdateTriggerRequestWorkflowInputsType0):
            workflow_inputs = self.workflow_inputs.to_dict()
        else:
            workflow_inputs = self.workflow_inputs

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "source": source,
            }
        )
        if agent is not UNSET:
            field_dict["agent"] = agent
        if enabled is not UNSET:
            field_dict["enabled"] = enabled
        if input_template is not UNSET:
            field_dict["input_template"] = input_template
        if on_busy is not UNSET:
            field_dict["on_busy"] = on_busy
        if workflow is not UNSET:
            field_dict["workflow"] = workflow
        if workflow_inputs is not UNSET:
            field_dict["workflow_inputs"] = workflow_inputs

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.trigger_source_input import TriggerSourceInput
        from ..models.update_trigger_request_workflow_inputs_type_0 import (
            UpdateTriggerRequestWorkflowInputsType0,
        )

        d = dict(src_dict)
        source = TriggerSourceInput.from_dict(d.pop("source"))

        def _parse_agent(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent = _parse_agent(d.pop("agent", UNSET))

        def _parse_enabled(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        enabled = _parse_enabled(d.pop("enabled", UNSET))

        def _parse_input_template(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        input_template = _parse_input_template(d.pop("input_template", UNSET))

        def _parse_on_busy(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        on_busy = _parse_on_busy(d.pop("on_busy", UNSET))

        def _parse_workflow(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        workflow = _parse_workflow(d.pop("workflow", UNSET))

        def _parse_workflow_inputs(
            data: object,
        ) -> None | Unset | UpdateTriggerRequestWorkflowInputsType0:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                workflow_inputs_type_0 = UpdateTriggerRequestWorkflowInputsType0.from_dict(data)

                return workflow_inputs_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | Unset | UpdateTriggerRequestWorkflowInputsType0, data)

        workflow_inputs = _parse_workflow_inputs(d.pop("workflow_inputs", UNSET))

        update_trigger_request = cls(
            source=source,
            agent=agent,
            enabled=enabled,
            input_template=input_template,
            on_busy=on_busy,
            workflow=workflow,
            workflow_inputs=workflow_inputs,
        )

        update_trigger_request.additional_properties = d
        return update_trigger_request

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
