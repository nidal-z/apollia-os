from enum import Enum


class IndexStateDto(str, Enum):
    ABSENT = "absent"
    ERROR = "error"
    INDEXING = "indexing"
    KEYWORDS_ONLY = "keywords_only"
    READY = "ready"

    def __str__(self) -> str:
        return str(self.value)
