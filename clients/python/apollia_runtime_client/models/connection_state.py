from enum import Enum


class ConnectionState(str, Enum):
    CONNECTED = "connected"
    DEGRADED = "degraded"
    NEEDS_REAUTH = "needs_reauth"
    STOPPED = "stopped"
    UNAVAILABLE = "unavailable"

    def __str__(self) -> str:
        return str(self.value)
