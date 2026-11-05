from enum import Enum


class EngineRatingReasonType6Code(str, Enum):
    NO_GPU = "no_gpu"

    def __str__(self) -> str:
        return str(self.value)
