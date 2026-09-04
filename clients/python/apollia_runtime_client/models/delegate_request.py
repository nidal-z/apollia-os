from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.delegate_request_input import DelegateRequestInput





T = TypeVar("T", bound="DelegateRequest")



@_attrs_define
class DelegateRequest:
    """ Request body for `POST /api/v1/a2a/delegate`.

        Attributes:
            input_ (DelegateRequestInput): JSON payload passed to the Worker Agent as input.
            skill_id (str): Target skill identifier (e.g. `"read-excel"`).
            timeout_secs (int | None | Unset): Delegation timeout in seconds (default: 120).
     """

    input_: DelegateRequestInput
    skill_id: str
    timeout_secs: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.delegate_request_input import DelegateRequestInput
        input_ = self.input_.to_dict()

        skill_id = self.skill_id

        timeout_secs: int | None | Unset
        if isinstance(self.timeout_secs, Unset):
            timeout_secs = UNSET
        else:
            timeout_secs = self.timeout_secs


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "input": input_,
            "skill_id": skill_id,
        })
        if timeout_secs is not UNSET:
            field_dict["timeout_secs"] = timeout_secs

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.delegate_request_input import DelegateRequestInput
        d = dict(src_dict)
        input_ = DelegateRequestInput.from_dict(d.pop("input"))




        skill_id = d.pop("skill_id")

        def _parse_timeout_secs(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        timeout_secs = _parse_timeout_secs(d.pop("timeout_secs", UNSET))


        delegate_request = cls(
            input_=input_,
            skill_id=skill_id,
            timeout_secs=timeout_secs,
        )


        delegate_request.additional_properties = d
        return delegate_request

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
