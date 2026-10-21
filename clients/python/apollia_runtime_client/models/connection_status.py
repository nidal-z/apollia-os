from enum import Enum


class ConnectionStatus(str, Enum):
    BLOCKED = "blocked"
    NEEDS_REPAIR = "needs_repair"
    PAUSED = "paused"
    WORKING = "working"

    def __str__(self) -> str:
        return str(self.value)
