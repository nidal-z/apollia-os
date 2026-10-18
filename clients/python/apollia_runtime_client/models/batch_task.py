from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.batch_output import BatchOutput


T = TypeVar("T", bound="BatchTask")


@_attrs_define
class BatchTask:
    """What the agent does on each item, and at the end, said at launch and
    recorded with the batch, beside its source, under `task`.

        Attributes:
            instruction (str | Unset): What the agent does with each item.
            outputs (list[BatchOutput] | Unset): The fields the answer of each item gives, one column of the results
                each. Empty: the answer is free text.
            synthesis (None | str | Unset): What the agent does once every item has run, on the table of their
                results: a ranking, a summary. Absent: no synthesis.
    """

    instruction: str | Unset = UNSET
    outputs: list[BatchOutput] | Unset = UNSET
    synthesis: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        instruction = self.instruction

        outputs: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.outputs, Unset):
            outputs = []
            for outputs_item_data in self.outputs:
                outputs_item = outputs_item_data.to_dict()
                outputs.append(outputs_item)

        synthesis: None | str | Unset
        if isinstance(self.synthesis, Unset):
            synthesis = UNSET
        else:
            synthesis = self.synthesis

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if instruction is not UNSET:
            field_dict["instruction"] = instruction
        if outputs is not UNSET:
            field_dict["outputs"] = outputs
        if synthesis is not UNSET:
            field_dict["synthesis"] = synthesis

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_output import BatchOutput

        d = dict(src_dict)
        instruction = d.pop("instruction", UNSET)

        _outputs = d.pop("outputs", UNSET)
        outputs: list[BatchOutput] | Unset = UNSET
        if _outputs is not UNSET:
            outputs = []
            for outputs_item_data in _outputs:
                outputs_item = BatchOutput.from_dict(outputs_item_data)

                outputs.append(outputs_item)

        def _parse_synthesis(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        synthesis = _parse_synthesis(d.pop("synthesis", UNSET))

        batch_task = cls(
            instruction=instruction,
            outputs=outputs,
            synthesis=synthesis,
        )

        batch_task.additional_properties = d
        return batch_task

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
