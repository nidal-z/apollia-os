from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.fs_hitl_decision_body_type_0 import FsHitlDecisionBodyType0
from ...models.fs_hitl_decision_body_type_1 import FsHitlDecisionBodyType1
from ...models.fs_hitl_decision_body_type_2 import FsHitlDecisionBodyType2
from ...models.fs_hitl_decision_body_type_3 import FsHitlDecisionBodyType3
from ...models.resolved_filesystem_approval import ResolvedFilesystemApproval
from ...types import Response


def _get_kwargs(
    request_id: str,
    *,
    body: FsHitlDecisionBodyType0
    | FsHitlDecisionBodyType1
    | FsHitlDecisionBodyType2
    | FsHitlDecisionBodyType3,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/approvals/filesystem/{request_id}".format(
            request_id=quote(str(request_id), safe=""),
        ),
    }

    if isinstance(body, FsHitlDecisionBodyType0):
        _kwargs["json"] = body.to_dict()
    elif isinstance(body, FsHitlDecisionBodyType1):
        _kwargs["json"] = body.to_dict()
    elif isinstance(body, FsHitlDecisionBodyType2):
        _kwargs["json"] = body.to_dict()
    else:
        _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ResolvedFilesystemApproval | None:
    if response.status_code == 200:
        response_200 = ResolvedFilesystemApproval.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ResolvedFilesystemApproval]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    request_id: str,
    *,
    client: AuthenticatedClient | Client,
    body: FsHitlDecisionBodyType0
    | FsHitlDecisionBodyType1
    | FsHitlDecisionBodyType2
    | FsHitlDecisionBodyType3,
) -> Response[ApiErrorBody | ResolvedFilesystemApproval]:
    """`POST /api/v1/approvals/filesystem/{request_id}`: answers a request to
    touch a file, which resumes the turn waiting on it.

    Args:
        request_id (str):
        body (FsHitlDecisionBodyType0 | FsHitlDecisionBodyType1 | FsHitlDecisionBodyType2 |
            FsHitlDecisionBodyType3): The user's answer to a request to touch a file, as the approval
            card sends
            it. The `decision` discriminant is snake_case; the breadth of an "always
            allow" is a separate `scope`, so earlier builds that sent only `op` and
            `level` keep working.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ResolvedFilesystemApproval]
    """

    kwargs = _get_kwargs(
        request_id=request_id,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    request_id: str,
    *,
    client: AuthenticatedClient | Client,
    body: FsHitlDecisionBodyType0
    | FsHitlDecisionBodyType1
    | FsHitlDecisionBodyType2
    | FsHitlDecisionBodyType3,
) -> ApiErrorBody | ResolvedFilesystemApproval | None:
    """`POST /api/v1/approvals/filesystem/{request_id}`: answers a request to
    touch a file, which resumes the turn waiting on it.

    Args:
        request_id (str):
        body (FsHitlDecisionBodyType0 | FsHitlDecisionBodyType1 | FsHitlDecisionBodyType2 |
            FsHitlDecisionBodyType3): The user's answer to a request to touch a file, as the approval
            card sends
            it. The `decision` discriminant is snake_case; the breadth of an "always
            allow" is a separate `scope`, so earlier builds that sent only `op` and
            `level` keep working.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ResolvedFilesystemApproval
    """

    return sync_detailed(
        request_id=request_id,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    request_id: str,
    *,
    client: AuthenticatedClient | Client,
    body: FsHitlDecisionBodyType0
    | FsHitlDecisionBodyType1
    | FsHitlDecisionBodyType2
    | FsHitlDecisionBodyType3,
) -> Response[ApiErrorBody | ResolvedFilesystemApproval]:
    """`POST /api/v1/approvals/filesystem/{request_id}`: answers a request to
    touch a file, which resumes the turn waiting on it.

    Args:
        request_id (str):
        body (FsHitlDecisionBodyType0 | FsHitlDecisionBodyType1 | FsHitlDecisionBodyType2 |
            FsHitlDecisionBodyType3): The user's answer to a request to touch a file, as the approval
            card sends
            it. The `decision` discriminant is snake_case; the breadth of an "always
            allow" is a separate `scope`, so earlier builds that sent only `op` and
            `level` keep working.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ResolvedFilesystemApproval]
    """

    kwargs = _get_kwargs(
        request_id=request_id,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    request_id: str,
    *,
    client: AuthenticatedClient | Client,
    body: FsHitlDecisionBodyType0
    | FsHitlDecisionBodyType1
    | FsHitlDecisionBodyType2
    | FsHitlDecisionBodyType3,
) -> ApiErrorBody | ResolvedFilesystemApproval | None:
    """`POST /api/v1/approvals/filesystem/{request_id}`: answers a request to
    touch a file, which resumes the turn waiting on it.

    Args:
        request_id (str):
        body (FsHitlDecisionBodyType0 | FsHitlDecisionBodyType1 | FsHitlDecisionBodyType2 |
            FsHitlDecisionBodyType3): The user's answer to a request to touch a file, as the approval
            card sends
            it. The `decision` discriminant is snake_case; the breadth of an "always
            allow" is a separate `scope`, so earlier builds that sent only `op` and
            `level` keep working.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ResolvedFilesystemApproval
    """

    return (
        await asyncio_detailed(
            request_id=request_id,
            client=client,
            body=body,
        )
    ).parsed
