from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="UpdateSessionBody")


@_attrs_define
class UpdateSessionBody:
    """Request body for `PATCH /api/v1/sessions/{id}`. An absent field keeps its
    value.

        Attributes:
            llm_backend (None | str | Unset): Model of the session: a backend, `<server>/<model>`, or the
                identifier of a language model of the model index. `null` returns to
                the model of the agent.
            profile_id (None | str | Unset): Profile the conversation designates; `null` returns it to the current
                default profile.
            system_prompt (None | str | Unset): New system prompt.
            tools (list[str] | None | Unset): New list of tools available in the session.
            use_profile (bool | None | Unset): Profile switch of the conversation: whether the built-in assistant
                receives the profile in its prompt.
    """

    llm_backend: None | str | Unset = UNSET
    profile_id: None | str | Unset = UNSET
    system_prompt: None | str | Unset = UNSET
    tools: list[str] | None | Unset = UNSET
    use_profile: bool | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        llm_backend: None | str | Unset
        if isinstance(self.llm_backend, Unset):
            llm_backend = UNSET
        else:
            llm_backend = self.llm_backend

        profile_id: None | str | Unset
        if isinstance(self.profile_id, Unset):
            profile_id = UNSET
        else:
            profile_id = self.profile_id

        system_prompt: None | str | Unset
        if isinstance(self.system_prompt, Unset):
            system_prompt = UNSET
        else:
            system_prompt = self.system_prompt

        tools: list[str] | None | Unset
        if isinstance(self.tools, Unset):
            tools = UNSET
        elif isinstance(self.tools, list):
            tools = self.tools

        else:
            tools = self.tools

        use_profile: bool | None | Unset
        if isinstance(self.use_profile, Unset):
            use_profile = UNSET
        else:
            use_profile = self.use_profile

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if llm_backend is not UNSET:
            field_dict["llm_backend"] = llm_backend
        if profile_id is not UNSET:
            field_dict["profile_id"] = profile_id
        if system_prompt is not UNSET:
            field_dict["system_prompt"] = system_prompt
        if tools is not UNSET:
            field_dict["tools"] = tools
        if use_profile is not UNSET:
            field_dict["use_profile"] = use_profile

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)

        def _parse_llm_backend(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        llm_backend = _parse_llm_backend(d.pop("llm_backend", UNSET))

        def _parse_profile_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        profile_id = _parse_profile_id(d.pop("profile_id", UNSET))

        def _parse_system_prompt(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        system_prompt = _parse_system_prompt(d.pop("system_prompt", UNSET))

        def _parse_tools(data: object) -> list[str] | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                tools_type_0 = cast(list[str], data)

                return tools_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(list[str] | None | Unset, data)

        tools = _parse_tools(d.pop("tools", UNSET))

        def _parse_use_profile(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        use_profile = _parse_use_profile(d.pop("use_profile", UNSET))

        update_session_body = cls(
            llm_backend=llm_backend,
            profile_id=profile_id,
            system_prompt=system_prompt,
            tools=tools,
            use_profile=use_profile,
        )

        update_session_body.additional_properties = d
        return update_session_body

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
