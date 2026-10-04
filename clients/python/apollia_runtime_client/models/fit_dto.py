from enum import Enum


class FitDto(str, Enum):
    FITS = "fits"
    MIGHT_FIT = "might_fit"
    TOO_LARGE = "too_large"

    def __str__(self) -> str:
        return str(self.value)
