from enum import Enum


class UnreadReasonDto(str, Enum):
    NOT_READ = "not_read"
    OCR_UNAVAILABLE = "ocr_unavailable"

    def __str__(self) -> str:
        return str(self.value)
