from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.inject_message_body_payload import InjectMessageBodyPayload





T = TypeVar("T", bound="InjectMessageBody")



@_attrs_define
class InjectMessageBody:
    """ Request body for `POST /api/v1/agents/:name/messages`.

        Attributes:
            payload (InjectMessageBodyPayload): Arbitrary JSON payload to deliver to the agent's inbox.
            from_ (None | str | Unset): Optional host identifier; the sender is recorded as `host:<id>` (or
                `host` when absent), so injected traffic is distinguishable in the audit.
     """

    payload: InjectMessageBodyPayload
    from_: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.inject_message_body_payload import InjectMessageBodyPayload
        payload = self.payload.to_dict()

        from_: None | str | Unset
        if isinstance(self.from_, Unset):
            from_ = UNSET
        else:
            from_ = self.from_


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "payload": payload,
        })
        if from_ is not UNSET:
            field_dict["from"] = from_

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.inject_message_body_payload import InjectMessageBodyPayload
        d = dict(src_dict)
        payload = InjectMessageBodyPayload.from_dict(d.pop("payload"))




        def _parse_from_(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        from_ = _parse_from_(d.pop("from", UNSET))


        inject_message_body = cls(
            payload=payload,
            from_=from_,
        )


        inject_message_body.additional_properties = d
        return inject_message_body

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
