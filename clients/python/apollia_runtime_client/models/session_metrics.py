from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.tool_stat_entry import ToolStatEntry


T = TypeVar("T", bound="SessionMetrics")


@_attrs_define
class SessionMetrics:
    """Aggregated metrics for a chat session.

    Accumulated by the [`ChatSessionManager`] on each `ExchangeComplete`.
    Backend-local: never persisted in SQLite, rebuilt from memory on restart
    (minimal slice; persistence is a future concern).

    `cost_usd` is `None` when the backend does not report pricing (e.g. local
    runner sidecar via `RunnerLlmBackend`).

        Attributes:
            budget_max_steps (int): Step budget in effect for this session (max steps).
            cache_read_input_tokens (int): Cached input tokens read (subset of `prompt_tokens`).
            cache_write_input_tokens (int): Cached input tokens written (subset of `prompt_tokens`).
            completion_tokens (int): Output tokens generated.
            context_tokens_used (int): Current context occupancy in tokens (prompt size of the last LLM call).
            context_window_tokens (int): Real context window of the active model, in tokens (`0` when unknown).
            exchanges_count (int): Number of exchanges completed.
            prompt_tokens (int): Input tokens consumed (prompt + cache reads + cache writes).
            session_id (str): Target session.
            steps_used (int): Steps consumed so far (exchanges producing assistant responses).
            tool_stats (list[ToolStatEntry]): Aggregated per-tool statistics.
            active_duration_ms (int | Unset): Cumulative active duration in milliseconds (LLM + tool execution time
                estimates).
            cost_usd (float | None | Unset): Estimated cost in USD (`None` for local backends).
            llm_backend (None | str | Unset): Backend/model name associated with the session (for pricing resolution).
            started_at (None | str | Unset): RFC-3339 timestamp of the first assistant response (session start, LLM time).
            updated_at (None | str | Unset): RFC-3339 timestamp of the last update.
    """

    budget_max_steps: int
    cache_read_input_tokens: int
    cache_write_input_tokens: int
    completion_tokens: int
    context_tokens_used: int
    context_window_tokens: int
    exchanges_count: int
    prompt_tokens: int
    session_id: str
    steps_used: int
    tool_stats: list[ToolStatEntry]
    active_duration_ms: int | Unset = UNSET
    cost_usd: float | None | Unset = UNSET
    llm_backend: None | str | Unset = UNSET
    started_at: None | str | Unset = UNSET
    updated_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        budget_max_steps = self.budget_max_steps

        cache_read_input_tokens = self.cache_read_input_tokens

        cache_write_input_tokens = self.cache_write_input_tokens

        completion_tokens = self.completion_tokens

        context_tokens_used = self.context_tokens_used

        context_window_tokens = self.context_window_tokens

        exchanges_count = self.exchanges_count

        prompt_tokens = self.prompt_tokens

        session_id = self.session_id

        steps_used = self.steps_used

        tool_stats = []
        for tool_stats_item_data in self.tool_stats:
            tool_stats_item = tool_stats_item_data.to_dict()
            tool_stats.append(tool_stats_item)

        active_duration_ms = self.active_duration_ms

        cost_usd: float | None | Unset
        if isinstance(self.cost_usd, Unset):
            cost_usd = UNSET
        else:
            cost_usd = self.cost_usd

        llm_backend: None | str | Unset
        if isinstance(self.llm_backend, Unset):
            llm_backend = UNSET
        else:
            llm_backend = self.llm_backend

        started_at: None | str | Unset
        if isinstance(self.started_at, Unset):
            started_at = UNSET
        else:
            started_at = self.started_at

        updated_at: None | str | Unset
        if isinstance(self.updated_at, Unset):
            updated_at = UNSET
        else:
            updated_at = self.updated_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "budget_max_steps": budget_max_steps,
                "cache_read_input_tokens": cache_read_input_tokens,
                "cache_write_input_tokens": cache_write_input_tokens,
                "completion_tokens": completion_tokens,
                "context_tokens_used": context_tokens_used,
                "context_window_tokens": context_window_tokens,
                "exchanges_count": exchanges_count,
                "prompt_tokens": prompt_tokens,
                "session_id": session_id,
                "steps_used": steps_used,
                "tool_stats": tool_stats,
            }
        )
        if active_duration_ms is not UNSET:
            field_dict["active_duration_ms"] = active_duration_ms
        if cost_usd is not UNSET:
            field_dict["cost_usd"] = cost_usd
        if llm_backend is not UNSET:
            field_dict["llm_backend"] = llm_backend
        if started_at is not UNSET:
            field_dict["started_at"] = started_at
        if updated_at is not UNSET:
            field_dict["updated_at"] = updated_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.tool_stat_entry import ToolStatEntry

        d = dict(src_dict)
        budget_max_steps = d.pop("budget_max_steps")

        cache_read_input_tokens = d.pop("cache_read_input_tokens")

        cache_write_input_tokens = d.pop("cache_write_input_tokens")

        completion_tokens = d.pop("completion_tokens")

        context_tokens_used = d.pop("context_tokens_used")

        context_window_tokens = d.pop("context_window_tokens")

        exchanges_count = d.pop("exchanges_count")

        prompt_tokens = d.pop("prompt_tokens")

        session_id = d.pop("session_id")

        steps_used = d.pop("steps_used")

        tool_stats = []
        _tool_stats = d.pop("tool_stats")
        for tool_stats_item_data in _tool_stats:
            tool_stats_item = ToolStatEntry.from_dict(tool_stats_item_data)

            tool_stats.append(tool_stats_item)

        active_duration_ms = d.pop("active_duration_ms", UNSET)

        def _parse_cost_usd(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        cost_usd = _parse_cost_usd(d.pop("cost_usd", UNSET))

        def _parse_llm_backend(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        llm_backend = _parse_llm_backend(d.pop("llm_backend", UNSET))

        def _parse_started_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        started_at = _parse_started_at(d.pop("started_at", UNSET))

        def _parse_updated_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        updated_at = _parse_updated_at(d.pop("updated_at", UNSET))

        session_metrics = cls(
            budget_max_steps=budget_max_steps,
            cache_read_input_tokens=cache_read_input_tokens,
            cache_write_input_tokens=cache_write_input_tokens,
            completion_tokens=completion_tokens,
            context_tokens_used=context_tokens_used,
            context_window_tokens=context_window_tokens,
            exchanges_count=exchanges_count,
            prompt_tokens=prompt_tokens,
            session_id=session_id,
            steps_used=steps_used,
            tool_stats=tool_stats,
            active_duration_ms=active_duration_ms,
            cost_usd=cost_usd,
            llm_backend=llm_backend,
            started_at=started_at,
            updated_at=updated_at,
        )

        session_metrics.additional_properties = d
        return session_metrics

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
