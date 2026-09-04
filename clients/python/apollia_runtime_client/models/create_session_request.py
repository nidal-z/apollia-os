from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="CreateSessionRequest")



@_attrs_define
class CreateSessionRequest:
    """ Request body for `POST /api/v1/sessions`.

        Attributes:
            mode (str): Session mode: `"libre"` or `"agent"`.
            agent_name (None | str | Unset): Agent name (required when `mode == "agent"`).
            project_id (None | str | Unset): Project to link this session to.
            system_prompt (None | str | Unset): Custom system prompt.
            tools (list[str] | Unset): List of tool names available in this session.
     """

    mode: str
    agent_name: None | str | Unset = UNSET
    project_id: None | str | Unset = UNSET
    system_prompt: None | str | Unset = UNSET
    tools: list[str] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        mode = self.mode

        agent_name: None | str | Unset
        if isinstance(self.agent_name, Unset):
            agent_name = UNSET
        else:
            agent_name = self.agent_name

        project_id: None | str | Unset
        if isinstance(self.project_id, Unset):
            project_id = UNSET
        else:
            project_id = self.project_id

        system_prompt: None | str | Unset
        if isinstance(self.system_prompt, Unset):
            system_prompt = UNSET
        else:
            system_prompt = self.system_prompt

        tools: list[str] | Unset = UNSET
        if not isinstance(self.tools, Unset):
            tools = self.tools




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "mode": mode,
        })
        if agent_name is not UNSET:
            field_dict["agent_name"] = agent_name
        if project_id is not UNSET:
            field_dict["project_id"] = project_id
        if system_prompt is not UNSET:
            field_dict["system_prompt"] = system_prompt
        if tools is not UNSET:
            field_dict["tools"] = tools

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        mode = d.pop("mode")

        def _parse_agent_name(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent_name = _parse_agent_name(d.pop("agent_name", UNSET))


        def _parse_project_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        project_id = _parse_project_id(d.pop("project_id", UNSET))


        def _parse_system_prompt(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        system_prompt = _parse_system_prompt(d.pop("system_prompt", UNSET))


        tools = cast(list[str], d.pop("tools", UNSET))


        create_session_request = cls(
            mode=mode,
            agent_name=agent_name,
            project_id=project_id,
            system_prompt=system_prompt,
            tools=tools,
        )


        create_session_request.additional_properties = d
        return create_session_request

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
