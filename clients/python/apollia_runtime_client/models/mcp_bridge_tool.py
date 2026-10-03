from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.mcp_bridge_tool_input_schema import McpBridgeToolInputSchema


T = TypeVar("T", bound="McpBridgeTool")


@_attrs_define
class McpBridgeTool:
    """One tool of the bridge, an MCP tool descriptor.

    Attributes:
        description (str): What the agent or the workflow does.
        input_schema (McpBridgeToolInputSchema): JSON Schema of the arguments.
        name (str): `agent__<id>` or `workflow__<package>_<workflow>`.
    """

    description: str
    input_schema: McpBridgeToolInputSchema
    name: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        description = self.description

        input_schema = self.input_schema.to_dict()

        name = self.name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "description": description,
                "inputSchema": input_schema,
                "name": name,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.mcp_bridge_tool_input_schema import McpBridgeToolInputSchema

        d = dict(src_dict)
        description = d.pop("description")

        input_schema = McpBridgeToolInputSchema.from_dict(d.pop("inputSchema"))

        name = d.pop("name")

        mcp_bridge_tool = cls(
            description=description,
            input_schema=input_schema,
            name=name,
        )

        mcp_bridge_tool.additional_properties = d
        return mcp_bridge_tool

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
