from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="AuditEventResponse")



@_attrs_define
class AuditEventResponse:
    """ A single audit event as returned by the API.

        Attributes:
            agent_id (str):
            id (str):
            input_hash (str):
            sandbox_profile (str):
            started_at (str):
            success (bool):
            task_id (str):
            tool_name (str):
            args_json (None | str | Unset): Arguments JSON complets de l'invocation.
            duration_ms (int | None | Unset):
            error_code (None | str | Unset):
            exit_code (int | None | Unset):
            run_id (None | str | Unset): Stable run identifier this invocation belongs to (the key `audit verify`
                uses). `null` for invocations recorded before run_id tracking (kept in the
                payload unconditionally so the schema is stable for automation).
            stderr (None | str | Unset): Error output of the tool, possibly truncated.
            stdout (None | str | Unset): Standard output of the tool, possibly truncated.
     """

    agent_id: str
    id: str
    input_hash: str
    sandbox_profile: str
    started_at: str
    success: bool
    task_id: str
    tool_name: str
    args_json: None | str | Unset = UNSET
    duration_ms: int | None | Unset = UNSET
    error_code: None | str | Unset = UNSET
    exit_code: int | None | Unset = UNSET
    run_id: None | str | Unset = UNSET
    stderr: None | str | Unset = UNSET
    stdout: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        agent_id = self.agent_id

        id = self.id

        input_hash = self.input_hash

        sandbox_profile = self.sandbox_profile

        started_at = self.started_at

        success = self.success

        task_id = self.task_id

        tool_name = self.tool_name

        args_json: None | str | Unset
        if isinstance(self.args_json, Unset):
            args_json = UNSET
        else:
            args_json = self.args_json

        duration_ms: int | None | Unset
        if isinstance(self.duration_ms, Unset):
            duration_ms = UNSET
        else:
            duration_ms = self.duration_ms

        error_code: None | str | Unset
        if isinstance(self.error_code, Unset):
            error_code = UNSET
        else:
            error_code = self.error_code

        exit_code: int | None | Unset
        if isinstance(self.exit_code, Unset):
            exit_code = UNSET
        else:
            exit_code = self.exit_code

        run_id: None | str | Unset
        if isinstance(self.run_id, Unset):
            run_id = UNSET
        else:
            run_id = self.run_id

        stderr: None | str | Unset
        if isinstance(self.stderr, Unset):
            stderr = UNSET
        else:
            stderr = self.stderr

        stdout: None | str | Unset
        if isinstance(self.stdout, Unset):
            stdout = UNSET
        else:
            stdout = self.stdout


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agent_id": agent_id,
            "id": id,
            "input_hash": input_hash,
            "sandbox_profile": sandbox_profile,
            "started_at": started_at,
            "success": success,
            "task_id": task_id,
            "tool_name": tool_name,
        })
        if args_json is not UNSET:
            field_dict["args_json"] = args_json
        if duration_ms is not UNSET:
            field_dict["duration_ms"] = duration_ms
        if error_code is not UNSET:
            field_dict["error_code"] = error_code
        if exit_code is not UNSET:
            field_dict["exit_code"] = exit_code
        if run_id is not UNSET:
            field_dict["run_id"] = run_id
        if stderr is not UNSET:
            field_dict["stderr"] = stderr
        if stdout is not UNSET:
            field_dict["stdout"] = stdout

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent_id = d.pop("agent_id")

        id = d.pop("id")

        input_hash = d.pop("input_hash")

        sandbox_profile = d.pop("sandbox_profile")

        started_at = d.pop("started_at")

        success = d.pop("success")

        task_id = d.pop("task_id")

        tool_name = d.pop("tool_name")

        def _parse_args_json(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        args_json = _parse_args_json(d.pop("args_json", UNSET))


        def _parse_duration_ms(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        duration_ms = _parse_duration_ms(d.pop("duration_ms", UNSET))


        def _parse_error_code(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error_code = _parse_error_code(d.pop("error_code", UNSET))


        def _parse_exit_code(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        exit_code = _parse_exit_code(d.pop("exit_code", UNSET))


        def _parse_run_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        run_id = _parse_run_id(d.pop("run_id", UNSET))


        def _parse_stderr(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        stderr = _parse_stderr(d.pop("stderr", UNSET))


        def _parse_stdout(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        stdout = _parse_stdout(d.pop("stdout", UNSET))


        audit_event_response = cls(
            agent_id=agent_id,
            id=id,
            input_hash=input_hash,
            sandbox_profile=sandbox_profile,
            started_at=started_at,
            success=success,
            task_id=task_id,
            tool_name=tool_name,
            args_json=args_json,
            duration_ms=duration_ms,
            error_code=error_code,
            exit_code=exit_code,
            run_id=run_id,
            stderr=stderr,
            stdout=stdout,
        )


        audit_event_response.additional_properties = d
        return audit_event_response

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
