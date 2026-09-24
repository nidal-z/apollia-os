from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.invoke_request_input import InvokeRequestInput


T = TypeVar("T", bound="InvokeRequest")


@_attrs_define
class InvokeRequest:
    """Request body for `POST /api/v1/a2a/invoke`.

    Attributes:
        input_ (InvokeRequestInput): JSON payload passed to the Worker Agent as input.
        skill_id (str): Target skill identifier (e.g. `"read-excel"`).
        caller (None | str | Unset): Caller name (Director Agent), used for observability.
        invocation_id (None | str | Unset): A name the client chooses for this invocation, so it can cancel it.

            This route is synchronous and answers only once the worker has
            finished, so the task id it returns arrives too late to cancel
            anything. A name supplied here is bound to the task as soon as the task
            is submitted, and `DELETE /api/v1/tasks/{id}` accepts it in place of the
            task id until this call returns. The invocation then answers `409` with
            a cancellation error instead of running to its timeout.

            A name already held by another live invocation is not bound; the call
            runs normally and the journal says so (`a2a.invocation_id.taken`).
        timeout_secs (int | None | Unset): Invocation timeout in seconds (default: 120).
    """

    input_: InvokeRequestInput
    skill_id: str
    caller: None | str | Unset = UNSET
    invocation_id: None | str | Unset = UNSET
    timeout_secs: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        input_ = self.input_.to_dict()

        skill_id = self.skill_id

        caller: None | str | Unset
        if isinstance(self.caller, Unset):
            caller = UNSET
        else:
            caller = self.caller

        invocation_id: None | str | Unset
        if isinstance(self.invocation_id, Unset):
            invocation_id = UNSET
        else:
            invocation_id = self.invocation_id

        timeout_secs: int | None | Unset
        if isinstance(self.timeout_secs, Unset):
            timeout_secs = UNSET
        else:
            timeout_secs = self.timeout_secs

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "input": input_,
                "skill_id": skill_id,
            }
        )
        if caller is not UNSET:
            field_dict["caller"] = caller
        if invocation_id is not UNSET:
            field_dict["invocation_id"] = invocation_id
        if timeout_secs is not UNSET:
            field_dict["timeout_secs"] = timeout_secs

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.invoke_request_input import InvokeRequestInput

        d = dict(src_dict)
        input_ = InvokeRequestInput.from_dict(d.pop("input"))

        skill_id = d.pop("skill_id")

        def _parse_caller(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        caller = _parse_caller(d.pop("caller", UNSET))

        def _parse_invocation_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        invocation_id = _parse_invocation_id(d.pop("invocation_id", UNSET))

        def _parse_timeout_secs(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        timeout_secs = _parse_timeout_secs(d.pop("timeout_secs", UNSET))

        invoke_request = cls(
            input_=input_,
            skill_id=skill_id,
            caller=caller,
            invocation_id=invocation_id,
            timeout_secs=timeout_secs,
        )

        invoke_request.additional_properties = d
        return invoke_request

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
