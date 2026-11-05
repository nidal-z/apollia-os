from enum import Enum


class EngineRatingReasonType5Code(str, Enum):
    NOT_BUILT_FOR_PLATFORM = "not_built_for_platform"

    def __str__(self) -> str:
        return str(self.value)
