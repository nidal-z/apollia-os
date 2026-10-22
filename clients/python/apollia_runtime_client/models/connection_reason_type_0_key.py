from enum import Enum


class ConnectionReasonType0Key(str, Enum):
    FAILING = "failing"
    FOLDER_NOT_FOUND = "folder_not_found"
    FORBIDDEN_BY_POLICY = "forbidden_by_policy"
    NOT_CHECKED = "not_checked"
    PASSWORD_REFUSED = "password_refused"
    SIGN_IN_AGAIN = "sign_in_again"
    UNREACHABLE = "unreachable"

    def __str__(self) -> str:
        return str(self.value)
