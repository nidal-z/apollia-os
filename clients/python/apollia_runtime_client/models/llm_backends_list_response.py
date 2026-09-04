from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.llm_backend_response import LlmBackendResponse





T = TypeVar("T", bound="LlmBackendsListResponse")



@_attrs_define
class LlmBackendsListResponse:
    """ Response body for `GET /api/v1/llm/backends`.

        Attributes:
            backends (list[LlmBackendResponse]): All configured backends.
     """

    backends: list[LlmBackendResponse]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.llm_backend_response import LlmBackendResponse
        backends = []
        for backends_item_data in self.backends:
            backends_item = backends_item_data.to_dict()
            backends.append(backends_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "backends": backends,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.llm_backend_response import LlmBackendResponse
        d = dict(src_dict)
        backends = []
        _backends = d.pop("backends")
        for backends_item_data in (_backends):
            backends_item = LlmBackendResponse.from_dict(backends_item_data)



            backends.append(backends_item)


        llm_backends_list_response = cls(
            backends=backends,
        )


        llm_backends_list_response.additional_properties = d
        return llm_backends_list_response

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
