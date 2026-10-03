from enum import Enum


class InputErrorCode(str, Enum):
    MALFORMED_DATE = "malformed_date"
    MISSING_FILE = "missing_file"
    NOT_AN_OBJECT = "not_an_object"
    NOT_A_CHOICE = "not_a_choice"
    NULL = "null"
    REQUIRED = "required"
    UNKNOWN = "unknown"
    WRONG_TYPE = "wrong_type"

    def __str__(self) -> str:
        return str(self.value)
