from enum import Enum


class EngineRatingReasonType8Code(str, Enum):
    NO_ROCM_RUNTIME = "no_rocm_runtime"

    def __str__(self) -> str:
        return str(self.value)
