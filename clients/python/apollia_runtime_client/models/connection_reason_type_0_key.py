from enum import Enum


class ConnectionReasonType0Key(str, Enum):
    ADMIN_APPROVAL_REQUIRED = "admin_approval_required"
    FAILING = "failing"
    FORBIDDEN_BY_POLICY = "forbidden_by_policy"
    NOT_CHECKED = "not_checked"
    SIGN_IN_AGAIN = "sign_in_again"
    UNREACHABLE = "unreachable"

    def __str__(self) -> str:
        return str(self.value)
