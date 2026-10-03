from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.trigger_definition_response_source_config import (
        TriggerDefinitionResponseSourceConfig,
    )
    from ..models.trigger_definition_response_workflow_inputs_type_0 import (
        TriggerDefinitionResponseWorkflowInputsType0,
    )


T = TypeVar("T", bound="TriggerDefinitionResponse")


@_attrs_define
class TriggerDefinitionResponse:
    """Response for CRUD operations returning a full definition.

    Attributes:
        created_at (str): Creation timestamp (ISO 8601).
        enabled (bool): Whether the trigger is active.
        id (str): Unique trigger identifier.
        on_busy (str): Policy when the agent is busy.
        source_config (TriggerDefinitionResponseSourceConfig): Source JSON configuration.
        source_type (str): Source type.
        target_kind (str): `"agent"` or `"workflow"`.
        updated_at (str): Last-modified timestamp (ISO 8601).
        agent (None | str | Unset): Target agent, for an agent target.
        input_template (None | str | Unset): The message of an agent target.
        workflow (None | str | Unset): Target workflow, for a workflow target.
        workflow_inputs (None | TriggerDefinitionResponseWorkflowInputsType0 | Unset): For a workflow target, one text
            template per input field.
    """

    created_at: str
    enabled: bool
    id: str
    on_busy: str
    source_config: TriggerDefinitionResponseSourceConfig
    source_type: str
    target_kind: str
    updated_at: str
    agent: None | str | Unset = UNSET
    input_template: None | str | Unset = UNSET
    workflow: None | str | Unset = UNSET
    workflow_inputs: None | TriggerDefinitionResponseWorkflowInputsType0 | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.trigger_definition_response_workflow_inputs_type_0 import (
            TriggerDefinitionResponseWorkflowInputsType0,
        )

        created_at = self.created_at

        enabled = self.enabled

        id = self.id

        on_busy = self.on_busy

        source_config = self.source_config.to_dict()

        source_type = self.source_type

        target_kind = self.target_kind

        updated_at = self.updated_at

        agent: None | str | Unset
        if isinstance(self.agent, Unset):
            agent = UNSET
        else:
            agent = self.agent

        input_template: None | str | Unset
        if isinstance(self.input_template, Unset):
            input_template = UNSET
        else:
            input_template = self.input_template

        workflow: None | str | Unset
        if isinstance(self.workflow, Unset):
            workflow = UNSET
        else:
            workflow = self.workflow

        workflow_inputs: dict[str, Any] | None | Unset
        if isinstance(self.workflow_inputs, Unset):
            workflow_inputs = UNSET
        elif isinstance(self.workflow_inputs, TriggerDefinitionResponseWorkflowInputsType0):
            workflow_inputs = self.workflow_inputs.to_dict()
        else:
            workflow_inputs = self.workflow_inputs

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "created_at": created_at,
                "enabled": enabled,
                "id": id,
                "on_busy": on_busy,
                "source_config": source_config,
                "source_type": source_type,
                "target_kind": target_kind,
                "updated_at": updated_at,
            }
        )
        if agent is not UNSET:
            field_dict["agent"] = agent
        if input_template is not UNSET:
            field_dict["input_template"] = input_template
        if workflow is not UNSET:
            field_dict["workflow"] = workflow
        if workflow_inputs is not UNSET:
            field_dict["workflow_inputs"] = workflow_inputs

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.trigger_definition_response_source_config import (
            TriggerDefinitionResponseSourceConfig,
        )
        from ..models.trigger_definition_response_workflow_inputs_type_0 import (
            TriggerDefinitionResponseWorkflowInputsType0,
        )

        d = dict(src_dict)
        created_at = d.pop("created_at")

        enabled = d.pop("enabled")

        id = d.pop("id")

        on_busy = d.pop("on_busy")

        source_config = TriggerDefinitionResponseSourceConfig.from_dict(d.pop("source_config"))

        source_type = d.pop("source_type")

        target_kind = d.pop("target_kind")

        updated_at = d.pop("updated_at")

        def _parse_agent(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent = _parse_agent(d.pop("agent", UNSET))

        def _parse_input_template(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        input_template = _parse_input_template(d.pop("input_template", UNSET))

        def _parse_workflow(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        workflow = _parse_workflow(d.pop("workflow", UNSET))

        def _parse_workflow_inputs(
            data: object,
        ) -> None | TriggerDefinitionResponseWorkflowInputsType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                workflow_inputs_type_0 = TriggerDefinitionResponseWorkflowInputsType0.from_dict(
                    data
                )

                return workflow_inputs_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | TriggerDefinitionResponseWorkflowInputsType0 | Unset, data)

        workflow_inputs = _parse_workflow_inputs(d.pop("workflow_inputs", UNSET))

        trigger_definition_response = cls(
            created_at=created_at,
            enabled=enabled,
            id=id,
            on_busy=on_busy,
            source_config=source_config,
            source_type=source_type,
            target_kind=target_kind,
            updated_at=updated_at,
            agent=agent,
            input_template=input_template,
            workflow=workflow,
            workflow_inputs=workflow_inputs,
        )

        trigger_definition_response.additional_properties = d
        return trigger_definition_response

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
