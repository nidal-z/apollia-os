from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.trigger_source_input import TriggerSourceInput





T = TypeVar("T", bound="CreateTriggerRequest")



@_attrs_define
class CreateTriggerRequest:
    """ Request body for `POST /api/v1/triggers`, trigger creation.

        Attributes:
            id (str): Unique trigger identifier.
            source (TriggerSourceInput): Trigger source description used in create/update requests.
            agent (None | str | Unset): Target agent.
            enabled (bool | None | Unset): Whether the trigger is active (default: `true`).
            input_template (None | str | Unset): Input message template.
            on_busy (None | str | Unset): Policy when the agent is busy (default: `"queue"`).
     """

    id: str
    source: TriggerSourceInput
    agent: None | str | Unset = UNSET
    enabled: bool | None | Unset = UNSET
    input_template: None | str | Unset = UNSET
    on_busy: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.trigger_source_input import TriggerSourceInput
        id = self.id

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


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "id": id,
            "source": source,
        })
        if agent is not UNSET:
            field_dict["agent"] = agent
        if enabled is not UNSET:
            field_dict["enabled"] = enabled
        if input_template is not UNSET:
            field_dict["input_template"] = input_template
        if on_busy is not UNSET:
            field_dict["on_busy"] = on_busy

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.trigger_source_input import TriggerSourceInput
        d = dict(src_dict)
        id = d.pop("id")

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


        create_trigger_request = cls(
            id=id,
            source=source,
            agent=agent,
            enabled=enabled,
            input_template=input_template,
            on_busy=on_busy,
        )


        create_trigger_request.additional_properties = d
        return create_trigger_request

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
