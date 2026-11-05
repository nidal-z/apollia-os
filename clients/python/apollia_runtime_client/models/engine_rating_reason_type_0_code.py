from enum import Enum


class EngineRatingReasonType0Code(str, Enum):
    DRIVER_TOO_OLD = "driver_too_old"

    def __str__(self) -> str:
        return str(self.value)
