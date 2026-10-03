from enum import Enum


class RewriteFallbackReason(str, Enum):
    CALLFAILED = "callFailed"
    EMPTYANSWER = "emptyAnswer"
    NOBACKEND = "noBackend"
    NOROUTER = "noRouter"

    def __str__(self) -> str:
        return str(self.value)
