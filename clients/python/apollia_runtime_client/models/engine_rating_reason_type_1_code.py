from enum import Enum


class EngineRatingReasonType1Code(str, Enum):
    ARCH_DROPPED_IN_CUDA13 = "arch_dropped_in_cuda13"

    def __str__(self) -> str:
        return str(self.value)
