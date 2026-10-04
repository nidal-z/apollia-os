from enum import Enum


class InputKind(str, Enum):
    BOOLEAN = "boolean"
    CHOICE = "choice"
    DATE = "date"
    FILE = "file"
    INTEGER = "integer"
    LIST = "list"
    LONG_TEXT = "long_text"
    NUMBER = "number"
    OTHER = "other"
    TEXT = "text"

    def __str__(self) -> str:
        return str(self.value)
