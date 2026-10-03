from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.server_log_level import ServerLogLevel
from ..models.server_log_source import ServerLogSource
from ..types import UNSET, Unset

T = TypeVar("T", bound="ServerLogLine")


@_attrs_define
class ServerLogLine:
    """One line of the server's logs.

    Attributes:
        level (ServerLogLevel): How serious a line is, in increasing order.
        seq (int): Position of the line since the runtime started; it keeps growing
            across a clear.
        source (ServerLogSource): Where a line comes from.
        timestamp (str): When the line was recorded, RFC 3339 in UTC.
        duration_ms (int | None | Unset): How long the server took to answer, in milliseconds. For a streamed
            answer, the time until the stream opened.
        endpoint (None | str | Unset): The path a request line asked for.
        message (None | str | Unset): The text an engine printed.
        method (None | str | Unset): The HTTP method of a request line.
        model (None | str | Unset): The model a request line named, when it named one.
        status (int | None | Unset): The status a request line was answered with.
    """

    level: ServerLogLevel
    seq: int
    source: ServerLogSource
    timestamp: str
    duration_ms: int | None | Unset = UNSET
    endpoint: None | str | Unset = UNSET
    message: None | str | Unset = UNSET
    method: None | str | Unset = UNSET
    model: None | str | Unset = UNSET
    status: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        level = self.level.value

        seq = self.seq

        source = self.source.value

        timestamp = self.timestamp

        duration_ms: int | None | Unset
        if isinstance(self.duration_ms, Unset):
            duration_ms = UNSET
        else:
            duration_ms = self.duration_ms

        endpoint: None | str | Unset
        if isinstance(self.endpoint, Unset):
            endpoint = UNSET
        else:
            endpoint = self.endpoint

        message: None | str | Unset
        if isinstance(self.message, Unset):
            message = UNSET
        else:
            message = self.message

        method: None | str | Unset
        if isinstance(self.method, Unset):
            method = UNSET
        else:
            method = self.method

        model: None | str | Unset
        if isinstance(self.model, Unset):
            model = UNSET
        else:
            model = self.model

        status: int | None | Unset
        if isinstance(self.status, Unset):
            status = UNSET
        else:
            status = self.status

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "level": level,
                "seq": seq,
                "source": source,
                "timestamp": timestamp,
            }
        )
        if duration_ms is not UNSET:
            field_dict["durationMs"] = duration_ms
        if endpoint is not UNSET:
            field_dict["endpoint"] = endpoint
        if message is not UNSET:
            field_dict["message"] = message
        if method is not UNSET:
            field_dict["method"] = method
        if model is not UNSET:
            field_dict["model"] = model
        if status is not UNSET:
            field_dict["status"] = status

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        level = ServerLogLevel(d.pop("level"))

        seq = d.pop("seq")

        source = ServerLogSource(d.pop("source"))

        timestamp = d.pop("timestamp")

        def _parse_duration_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        duration_ms = _parse_duration_ms(d.pop("durationMs", UNSET))

        def _parse_endpoint(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        endpoint = _parse_endpoint(d.pop("endpoint", UNSET))

        def _parse_message(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        message = _parse_message(d.pop("message", UNSET))

        def _parse_method(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        method = _parse_method(d.pop("method", UNSET))

        def _parse_model(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        model = _parse_model(d.pop("model", UNSET))

        def _parse_status(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        status = _parse_status(d.pop("status", UNSET))

        server_log_line = cls(
            level=level,
            seq=seq,
            source=source,
            timestamp=timestamp,
            duration_ms=duration_ms,
            endpoint=endpoint,
            message=message,
            method=method,
            model=model,
            status=status,
        )

        server_log_line.additional_properties = d
        return server_log_line

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
