from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define

from ..models.context_overflow_setting import ContextOverflowSetting
from ..types import UNSET, Unset

T = TypeVar("T", bound="SessionSettings")


@_attrs_define
class SessionSettings:
    """The settings of one conversation. Every field is optional: one left out
    is inherited from the agent. An unknown field is refused.

        Attributes:
            context_overflow (ContextOverflowSetting | None | Unset):
            disabled_accesses (list[str] | Unset): Accesses of the agent switched off for this conversation.
            instructions (None | str | Unset): Instructions added to the mission of the agent for this conversation.
            max_tokens (int | None | Unset): Upper bound on the length of an answer, in tokens.
            only_accesses (list[str] | Unset): Accesses the conversation is limited to. When the list is not empty,
                of the accesses of the agent only those it names are kept, and those
                it names that the agent does not hold are proposed: each of their
                calls asks for consent.
            preset (int | None | Unset): Preset chosen for the conversation, a preset of its model. It takes
                the place of the agent's preset.
            profile_access (bool | None | Unset): For an installed agent, whether it may read the user profile in this
                conversation, whatever its own profile source says. Left out, the
                source of the agent decides. The profile is read by a tool and never
                placed in the prompt of an installed agent.
            proposed_accesses (list[str] | Unset): Accesses the agent does not hold, proposed for this conversation:
                the model may call them and each call asks for consent.
            reasoning_effort (None | str | Unset): Level of reasoning (`low`, `medium`, `high`), for a model that reads it.
            temperature (float | None | Unset): Sampling temperature, between 0 and 2.
            thinking (bool | None | Unset): Reasoning on or off, for a model that reads it.
    """

    context_overflow: ContextOverflowSetting | None | Unset = UNSET
    disabled_accesses: list[str] | Unset = UNSET
    instructions: None | str | Unset = UNSET
    max_tokens: int | None | Unset = UNSET
    only_accesses: list[str] | Unset = UNSET
    preset: int | None | Unset = UNSET
    profile_access: bool | None | Unset = UNSET
    proposed_accesses: list[str] | Unset = UNSET
    reasoning_effort: None | str | Unset = UNSET
    temperature: float | None | Unset = UNSET
    thinking: bool | None | Unset = UNSET

    def to_dict(self) -> dict[str, Any]:
        context_overflow: None | str | Unset
        if isinstance(self.context_overflow, Unset):
            context_overflow = UNSET
        elif isinstance(self.context_overflow, ContextOverflowSetting):
            context_overflow = self.context_overflow.value
        else:
            context_overflow = self.context_overflow

        disabled_accesses: list[str] | Unset = UNSET
        if not isinstance(self.disabled_accesses, Unset):
            disabled_accesses = self.disabled_accesses

        instructions: None | str | Unset
        if isinstance(self.instructions, Unset):
            instructions = UNSET
        else:
            instructions = self.instructions

        max_tokens: int | None | Unset
        if isinstance(self.max_tokens, Unset):
            max_tokens = UNSET
        else:
            max_tokens = self.max_tokens

        only_accesses: list[str] | Unset = UNSET
        if not isinstance(self.only_accesses, Unset):
            only_accesses = self.only_accesses

        preset: int | None | Unset
        if isinstance(self.preset, Unset):
            preset = UNSET
        else:
            preset = self.preset

        profile_access: bool | None | Unset
        if isinstance(self.profile_access, Unset):
            profile_access = UNSET
        else:
            profile_access = self.profile_access

        proposed_accesses: list[str] | Unset = UNSET
        if not isinstance(self.proposed_accesses, Unset):
            proposed_accesses = self.proposed_accesses

        reasoning_effort: None | str | Unset
        if isinstance(self.reasoning_effort, Unset):
            reasoning_effort = UNSET
        else:
            reasoning_effort = self.reasoning_effort

        temperature: float | None | Unset
        if isinstance(self.temperature, Unset):
            temperature = UNSET
        else:
            temperature = self.temperature

        thinking: bool | None | Unset
        if isinstance(self.thinking, Unset):
            thinking = UNSET
        else:
            thinking = self.thinking

        field_dict: dict[str, Any] = {}

        field_dict.update({})
        if context_overflow is not UNSET:
            field_dict["context_overflow"] = context_overflow
        if disabled_accesses is not UNSET:
            field_dict["disabled_accesses"] = disabled_accesses
        if instructions is not UNSET:
            field_dict["instructions"] = instructions
        if max_tokens is not UNSET:
            field_dict["max_tokens"] = max_tokens
        if only_accesses is not UNSET:
            field_dict["only_accesses"] = only_accesses
        if preset is not UNSET:
            field_dict["preset"] = preset
        if profile_access is not UNSET:
            field_dict["profile_access"] = profile_access
        if proposed_accesses is not UNSET:
            field_dict["proposed_accesses"] = proposed_accesses
        if reasoning_effort is not UNSET:
            field_dict["reasoning_effort"] = reasoning_effort
        if temperature is not UNSET:
            field_dict["temperature"] = temperature
        if thinking is not UNSET:
            field_dict["thinking"] = thinking

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)

        def _parse_context_overflow(data: object) -> ContextOverflowSetting | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                context_overflow_type_1 = ContextOverflowSetting(data)

                return context_overflow_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(ContextOverflowSetting | None | Unset, data)

        context_overflow = _parse_context_overflow(d.pop("context_overflow", UNSET))

        disabled_accesses = cast(list[str], d.pop("disabled_accesses", UNSET))

        def _parse_instructions(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        instructions = _parse_instructions(d.pop("instructions", UNSET))

        def _parse_max_tokens(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        max_tokens = _parse_max_tokens(d.pop("max_tokens", UNSET))

        only_accesses = cast(list[str], d.pop("only_accesses", UNSET))

        def _parse_preset(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        preset = _parse_preset(d.pop("preset", UNSET))

        def _parse_profile_access(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        profile_access = _parse_profile_access(d.pop("profile_access", UNSET))

        proposed_accesses = cast(list[str], d.pop("proposed_accesses", UNSET))

        def _parse_reasoning_effort(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reasoning_effort = _parse_reasoning_effort(d.pop("reasoning_effort", UNSET))

        def _parse_temperature(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        temperature = _parse_temperature(d.pop("temperature", UNSET))

        def _parse_thinking(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        thinking = _parse_thinking(d.pop("thinking", UNSET))

        session_settings = cls(
            context_overflow=context_overflow,
            disabled_accesses=disabled_accesses,
            instructions=instructions,
            max_tokens=max_tokens,
            only_accesses=only_accesses,
            preset=preset,
            profile_access=profile_access,
            proposed_accesses=proposed_accesses,
            reasoning_effort=reasoning_effort,
            temperature=temperature,
            thinking=thinking,
        )

        return session_settings
