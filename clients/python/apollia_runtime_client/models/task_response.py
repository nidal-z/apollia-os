from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.task_response_result_type_0 import TaskResponseResultType0
  from ..models.task_response_token_budget_type_0 import TaskResponseTokenBudgetType0





T = TypeVar("T", bound="TaskResponse")



@_attrs_define
class TaskResponse:
    """ Response body for task operations.

        Attributes:
            status (str): Current task status.
            task_id (str): Unique task identifier (UUID v4).
            error (None | str | Unset): Error message (present when failed).
            error_code (None | str | Unset): Structured failure code parsed from the error (e.g. `BAD_MESSAGE`), so
                automation can branch on the code without string-matching the message.
            result (None | TaskResponseResultType0 | Unset): Task result payload (present when completed).
            run_id (None | str | Unset): Stable run identifier this task belongs to, used by `audit verify`.
                Kept unconditionally so the schema is stable for automation.
            token_budget (None | TaskResponseTokenBudgetType0 | Unset): Token budget accumulated over all LLM calls for this
                task.
     """

    status: str
    task_id: str
    error: None | str | Unset = UNSET
    error_code: None | str | Unset = UNSET
    result: None | TaskResponseResultType0 | Unset = UNSET
    run_id: None | str | Unset = UNSET
    token_budget: None | TaskResponseTokenBudgetType0 | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.task_response_result_type_0 import TaskResponseResultType0
        from ..models.task_response_token_budget_type_0 import TaskResponseTokenBudgetType0
        status = self.status

        task_id = self.task_id

        error: None | str | Unset
        if isinstance(self.error, Unset):
            error = UNSET
        else:
            error = self.error

        error_code: None | str | Unset
        if isinstance(self.error_code, Unset):
            error_code = UNSET
        else:
            error_code = self.error_code

        result: dict[str, Any] | None | Unset
        if isinstance(self.result, Unset):
            result = UNSET
        elif isinstance(self.result, TaskResponseResultType0):
            result = self.result.to_dict()
        else:
            result = self.result

        run_id: None | str | Unset
        if isinstance(self.run_id, Unset):
            run_id = UNSET
        else:
            run_id = self.run_id

        token_budget: dict[str, Any] | None | Unset
        if isinstance(self.token_budget, Unset):
            token_budget = UNSET
        elif isinstance(self.token_budget, TaskResponseTokenBudgetType0):
            token_budget = self.token_budget.to_dict()
        else:
            token_budget = self.token_budget


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "status": status,
            "task_id": task_id,
        })
        if error is not UNSET:
            field_dict["error"] = error
        if error_code is not UNSET:
            field_dict["error_code"] = error_code
        if result is not UNSET:
            field_dict["result"] = result
        if run_id is not UNSET:
            field_dict["run_id"] = run_id
        if token_budget is not UNSET:
            field_dict["token_budget"] = token_budget

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.task_response_result_type_0 import TaskResponseResultType0
        from ..models.task_response_token_budget_type_0 import TaskResponseTokenBudgetType0
        d = dict(src_dict)
        status = d.pop("status")

        task_id = d.pop("task_id")

        def _parse_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error = _parse_error(d.pop("error", UNSET))


        def _parse_error_code(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error_code = _parse_error_code(d.pop("error_code", UNSET))


        def _parse_result(data: object) -> None | TaskResponseResultType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                result_type_0 = TaskResponseResultType0.from_dict(data)



                return result_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | TaskResponseResultType0 | Unset, data)

        result = _parse_result(d.pop("result", UNSET))


        def _parse_run_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        run_id = _parse_run_id(d.pop("run_id", UNSET))


        def _parse_token_budget(data: object) -> None | TaskResponseTokenBudgetType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                token_budget_type_0 = TaskResponseTokenBudgetType0.from_dict(data)



                return token_budget_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | TaskResponseTokenBudgetType0 | Unset, data)

        token_budget = _parse_token_budget(d.pop("token_budget", UNSET))


        task_response = cls(
            status=status,
            task_id=task_id,
            error=error,
            error_code=error_code,
            result=result,
            run_id=run_id,
            token_budget=token_budget,
        )


        task_response.additional_properties = d
        return task_response

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
