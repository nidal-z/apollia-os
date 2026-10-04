from enum import Enum


class IssueReason(str, Enum):
    AGENT_NOT_INSTALLED = "agent_not_installed"
    FAILED = "failed"
    MISSING = "missing"
    NOT_CARRIED_OVER = "not_carried_over"
    UNREADABLE = "unreadable"
    WRONG_KIND = "wrong_kind"

    def __str__(self) -> str:
        return str(self.value)
