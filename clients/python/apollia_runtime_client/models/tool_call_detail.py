from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.tool_call_detail_arguments_type_0 import ToolCallDetailArgumentsType0
    from ..models.tool_call_detail_output_type_0 import ToolCallDetailOutputType0


T = TypeVar("T", bound="ToolCallDetail")


@_attrs_define
class ToolCallDetail:
    """The arguments and the output of one tool call.

    Attributes:
        tool_call_id (str): Identifier of the call within its run.
        arguments (None | ToolCallDetailArgumentsType0 | Unset): Full arguments of the call. `None` when no model
            response of the run
            carries this call, as for a tool an agent's own code called.
        output (None | ToolCallDetailOutputType0 | Unset): Full output of the call. `None` when the run captured none.
        status (None | str | Unset): How the call ended, as the capture wrote it: `success`, `error` or
            `rejected`.
    """

    tool_call_id: str
    arguments: None | ToolCallDetailArgumentsType0 | Unset = UNSET
    output: None | ToolCallDetailOutputType0 | Unset = UNSET
    status: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.tool_call_detail_arguments_type_0 import ToolCallDetailArgumentsType0
        from ..models.tool_call_detail_output_type_0 import ToolCallDetailOutputType0

        tool_call_id = self.tool_call_id

        arguments: dict[str, Any] | None | Unset
        if isinstance(self.arguments, Unset):
            arguments = UNSET
        elif isinstance(self.arguments, ToolCallDetailArgumentsType0):
            arguments = self.arguments.to_dict()
        else:
            arguments = self.arguments

        output: dict[str, Any] | None | Unset
        if isinstance(self.output, Unset):
            output = UNSET
        elif isinstance(self.output, ToolCallDetailOutputType0):
            output = self.output.to_dict()
        else:
            output = self.output

        status: None | str | Unset
        if isinstance(self.status, Unset):
            status = UNSET
        else:
            status = self.status

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "tool_call_id": tool_call_id,
            }
        )
        if arguments is not UNSET:
            field_dict["arguments"] = arguments
        if output is not UNSET:
            field_dict["output"] = output
        if status is not UNSET:
            field_dict["status"] = status

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.tool_call_detail_arguments_type_0 import ToolCallDetailArgumentsType0
        from ..models.tool_call_detail_output_type_0 import ToolCallDetailOutputType0

        d = dict(src_dict)
        tool_call_id = d.pop("tool_call_id")

        def _parse_arguments(data: object) -> None | ToolCallDetailArgumentsType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                arguments_type_0 = ToolCallDetailArgumentsType0.from_dict(data)

                return arguments_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | ToolCallDetailArgumentsType0 | Unset, data)

        arguments = _parse_arguments(d.pop("arguments", UNSET))

        def _parse_output(data: object) -> None | ToolCallDetailOutputType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                output_type_0 = ToolCallDetailOutputType0.from_dict(data)

                return output_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | ToolCallDetailOutputType0 | Unset, data)

        output = _parse_output(d.pop("output", UNSET))

        def _parse_status(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        status = _parse_status(d.pop("status", UNSET))

        tool_call_detail = cls(
            tool_call_id=tool_call_id,
            arguments=arguments,
            output=output,
            status=status,
        )

        tool_call_detail.additional_properties = d
        return tool_call_detail

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
