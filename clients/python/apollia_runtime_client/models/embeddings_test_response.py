from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="EmbeddingsTestResponse")


@_attrs_define
class EmbeddingsTestResponse:
    """What the embeddings model answered to the test.

    Attributes:
        dimensions (int): Number of values of the vector it returned.
        elapsed_ms (int): Time from the request to the vector, loading included, in milliseconds.
        model (str): Identifier of the model that answered, an entry of the registry.
    """

    dimensions: int
    elapsed_ms: int
    model: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        dimensions = self.dimensions

        elapsed_ms = self.elapsed_ms

        model = self.model

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "dimensions": dimensions,
                "elapsed_ms": elapsed_ms,
                "model": model,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        dimensions = d.pop("dimensions")

        elapsed_ms = d.pop("elapsed_ms")

        model = d.pop("model")

        embeddings_test_response = cls(
            dimensions=dimensions,
            elapsed_ms=elapsed_ms,
            model=model,
        )

        embeddings_test_response.additional_properties = d
        return embeddings_test_response

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
