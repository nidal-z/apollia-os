from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.submit_task_request_input import SubmitTaskRequestInput
  from ..models.submit_task_request_run_options import SubmitTaskRequestRunOptions





T = TypeVar("T", bound="SubmitTaskRequest")



@_attrs_define
class SubmitTaskRequest:
    """ Request body for `POST /api/v1/tasks`.

        Attributes:
            agent_id (str): Identifier of the target agent.
            input_ (SubmitTaskRequestInput): Free-form JSON input for the task.
            run_options (SubmitTaskRequestRunOptions | Unset): Per-run control options (plan-gate / autonomy overrides).
     """

    agent_id: str
    input_: SubmitTaskRequestInput
    run_options: SubmitTaskRequestRunOptions | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.submit_task_request_input import SubmitTaskRequestInput
        from ..models.submit_task_request_run_options import SubmitTaskRequestRunOptions
        agent_id = self.agent_id

        input_ = self.input_.to_dict()

        run_options: dict[str, Any] | Unset = UNSET
        if not isinstance(self.run_options, Unset):
            run_options = self.run_options.to_dict()


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agent_id": agent_id,
            "input": input_,
        })
        if run_options is not UNSET:
            field_dict["run_options"] = run_options

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.submit_task_request_input import SubmitTaskRequestInput
        from ..models.submit_task_request_run_options import SubmitTaskRequestRunOptions
        d = dict(src_dict)
        agent_id = d.pop("agent_id")

        input_ = SubmitTaskRequestInput.from_dict(d.pop("input"))




        _run_options = d.pop("run_options", UNSET)
        run_options: SubmitTaskRequestRunOptions | Unset
        if isinstance(_run_options,  Unset):
            run_options = UNSET
        else:
            run_options = SubmitTaskRequestRunOptions.from_dict(_run_options)




        submit_task_request = cls(
            agent_id=agent_id,
            input_=input_,
            run_options=run_options,
        )


        submit_task_request.additional_properties = d
        return submit_task_request

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
