from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.input_error_code import InputErrorCode
from ..types import UNSET, Unset

T = TypeVar("T", bound="WorkflowErrorResponse")


@_attrs_define
class WorkflowErrorResponse:
    """Error body of the workflow routes.

    Attributes:
        error (str): What went wrong.
        code (InputErrorCode | None | Unset):
        field (None | str | Unset): The input at fault, when an input was refused.
    """

    error: str
    code: InputErrorCode | None | Unset = UNSET
    field: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        error = self.error

        code: None | str | Unset
        if isinstance(self.code, Unset):
            code = UNSET
        elif isinstance(self.code, InputErrorCode):
            code = self.code.value
        else:
            code = self.code

        field: None | str | Unset
        if isinstance(self.field, Unset):
            field = UNSET
        else:
            field = self.field

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "error": error,
            }
        )
        if code is not UNSET:
            field_dict["code"] = code
        if field is not UNSET:
            field_dict["field"] = field

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        error = d.pop("error")

        def _parse_code(data: object) -> InputErrorCode | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                code_type_1 = InputErrorCode(data)

                return code_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(InputErrorCode | None | Unset, data)

        code = _parse_code(d.pop("code", UNSET))

        def _parse_field(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        field = _parse_field(d.pop("field", UNSET))

        workflow_error_response = cls(
            error=error,
            code=code,
            field=field,
        )

        workflow_error_response.additional_properties = d
        return workflow_error_response

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
