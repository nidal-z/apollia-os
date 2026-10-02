from enum import Enum


class PackageNoticeType3Code(str, Enum):
    SERVER_ALREADY_PRESENT = "server_already_present"

    def __str__(self) -> str:
        return str(self.value)
