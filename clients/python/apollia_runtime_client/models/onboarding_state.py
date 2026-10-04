from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.onboarding_phase import OnboardingPhase
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.onboarding_stats import OnboardingStats


T = TypeVar("T", bound="OnboardingState")


@_attrs_define
class OnboardingState:
    """Full onboarding state maintained server-side and persisted in UserMemory.

    Attributes:
        completed (bool): `true` when the phase has reached `Done`.
        llm_configured (bool): `true` when at least one LLM backend has been configured.
        mandatory_complete (bool): `true` when the mandatory name/role fields have been collected.
        phase (OnboardingPhase): The six ordered phases of the onboarding flow.
        skipped (bool): `true` if the user explicitly skipped the onboarding.
        stats (OnboardingStats): Cumulative statistics accumulated over the full onboarding flow.
        stt_configured (bool): `true` when the STT engine has been configured and tested.
        topics_covered (list[str]): Topics covered so far.
        voice_enabled (bool): `true` when the user has enabled voice commands.
        companion_session_id (None | str | Unset): Session ID of the active Companion chat, if any.
        completed_at (None | str | Unset): ISO 8601 timestamp when the phase reached `Done`.
        started_at (None | str | Unset): ISO 8601 timestamp of the first phase transition (Welcome to AiSetup).
    """

    completed: bool
    llm_configured: bool
    mandatory_complete: bool
    phase: OnboardingPhase
    skipped: bool
    stats: OnboardingStats
    stt_configured: bool
    topics_covered: list[str]
    voice_enabled: bool
    companion_session_id: None | str | Unset = UNSET
    completed_at: None | str | Unset = UNSET
    started_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        completed = self.completed

        llm_configured = self.llm_configured

        mandatory_complete = self.mandatory_complete

        phase = self.phase.value

        skipped = self.skipped

        stats = self.stats.to_dict()

        stt_configured = self.stt_configured

        topics_covered = self.topics_covered

        voice_enabled = self.voice_enabled

        companion_session_id: None | str | Unset
        if isinstance(self.companion_session_id, Unset):
            companion_session_id = UNSET
        else:
            companion_session_id = self.companion_session_id

        completed_at: None | str | Unset
        if isinstance(self.completed_at, Unset):
            completed_at = UNSET
        else:
            completed_at = self.completed_at

        started_at: None | str | Unset
        if isinstance(self.started_at, Unset):
            started_at = UNSET
        else:
            started_at = self.started_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "completed": completed,
                "llm_configured": llm_configured,
                "mandatory_complete": mandatory_complete,
                "phase": phase,
                "skipped": skipped,
                "stats": stats,
                "stt_configured": stt_configured,
                "topics_covered": topics_covered,
                "voice_enabled": voice_enabled,
            }
        )
        if companion_session_id is not UNSET:
            field_dict["companion_session_id"] = companion_session_id
        if completed_at is not UNSET:
            field_dict["completed_at"] = completed_at
        if started_at is not UNSET:
            field_dict["started_at"] = started_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.onboarding_stats import OnboardingStats

        d = dict(src_dict)
        completed = d.pop("completed")

        llm_configured = d.pop("llm_configured")

        mandatory_complete = d.pop("mandatory_complete")

        phase = OnboardingPhase(d.pop("phase"))

        skipped = d.pop("skipped")

        stats = OnboardingStats.from_dict(d.pop("stats"))

        stt_configured = d.pop("stt_configured")

        topics_covered = cast(list[str], d.pop("topics_covered"))

        voice_enabled = d.pop("voice_enabled")

        def _parse_companion_session_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        companion_session_id = _parse_companion_session_id(d.pop("companion_session_id", UNSET))

        def _parse_completed_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        completed_at = _parse_completed_at(d.pop("completed_at", UNSET))

        def _parse_started_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        started_at = _parse_started_at(d.pop("started_at", UNSET))

        onboarding_state = cls(
            completed=completed,
            llm_configured=llm_configured,
            mandatory_complete=mandatory_complete,
            phase=phase,
            skipped=skipped,
            stats=stats,
            stt_configured=stt_configured,
            topics_covered=topics_covered,
            voice_enabled=voice_enabled,
            companion_session_id=companion_session_id,
            completed_at=completed_at,
            started_at=started_at,
        )

        onboarding_state.additional_properties = d
        return onboarding_state

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
