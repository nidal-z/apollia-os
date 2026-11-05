from enum import Enum


class EngineRatingReasonType7Code(str, Enum):
    ENGINE_SEES_NO_DEVICE = "engine_sees_no_device"

    def __str__(self) -> str:
        return str(self.value)
