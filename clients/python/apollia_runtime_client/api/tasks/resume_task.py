from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.resume_request import ResumeRequest
from ...models.resume_response import ResumeResponse
from ...types import Response


def _get_kwargs(
    id: str,
    *,
    body: ResumeRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/tasks/{id}/resume".format(
            id=quote(str(id), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ResumeResponse | None:
    if response.status_code == 200:
        response_200 = ResumeResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = ApiErrorBody.from_dict(response.json())

        return response_409

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ResumeResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: ResumeRequest,
) -> Response[ApiErrorBody | ResumeResponse]:
    """Handler for `POST /api/v1/tasks/{id}/resume`.

     Validates that the task is in `input_required` status, persists the human
    decision to SQLite, emits `RuntimeEvent::TaskResumed` on the EventBus,
    and rebuilds the enriched `AIPTask` for the ORIA relaunch.

    ## HTTP codes
    - `200 OK`, resume recorded successfully
    - `404 Not Found`, task unknown to the HITL system
    - `409 Conflict`, task known but not in `input_required` status
    - `503 Service Unavailable`, HITL not configured (`task_repository` absent)
    - `500 Internal Server Error`, SQLite or internal error

    Args:
        id (str):
        body (ResumeRequest): Request body for `POST /api/v1/tasks/{id}/resume`.

            The operator submits a decision (`approved`) and an optional reason.
            The `approved` field is mandatory; omitting it produces HTTP 422.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ResumeResponse]
    """

    kwargs = _get_kwargs(
        id=id,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: ResumeRequest,
) -> ApiErrorBody | ResumeResponse | None:
    """Handler for `POST /api/v1/tasks/{id}/resume`.

     Validates that the task is in `input_required` status, persists the human
    decision to SQLite, emits `RuntimeEvent::TaskResumed` on the EventBus,
    and rebuilds the enriched `AIPTask` for the ORIA relaunch.

    ## HTTP codes
    - `200 OK`, resume recorded successfully
    - `404 Not Found`, task unknown to the HITL system
    - `409 Conflict`, task known but not in `input_required` status
    - `503 Service Unavailable`, HITL not configured (`task_repository` absent)
    - `500 Internal Server Error`, SQLite or internal error

    Args:
        id (str):
        body (ResumeRequest): Request body for `POST /api/v1/tasks/{id}/resume`.

            The operator submits a decision (`approved`) and an optional reason.
            The `approved` field is mandatory; omitting it produces HTTP 422.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ResumeResponse
    """

    return sync_detailed(
        id=id,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: ResumeRequest,
) -> Response[ApiErrorBody | ResumeResponse]:
    """Handler for `POST /api/v1/tasks/{id}/resume`.

     Validates that the task is in `input_required` status, persists the human
    decision to SQLite, emits `RuntimeEvent::TaskResumed` on the EventBus,
    and rebuilds the enriched `AIPTask` for the ORIA relaunch.

    ## HTTP codes
    - `200 OK`, resume recorded successfully
    - `404 Not Found`, task unknown to the HITL system
    - `409 Conflict`, task known but not in `input_required` status
    - `503 Service Unavailable`, HITL not configured (`task_repository` absent)
    - `500 Internal Server Error`, SQLite or internal error

    Args:
        id (str):
        body (ResumeRequest): Request body for `POST /api/v1/tasks/{id}/resume`.

            The operator submits a decision (`approved`) and an optional reason.
            The `approved` field is mandatory; omitting it produces HTTP 422.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ResumeResponse]
    """

    kwargs = _get_kwargs(
        id=id,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: ResumeRequest,
) -> ApiErrorBody | ResumeResponse | None:
    """Handler for `POST /api/v1/tasks/{id}/resume`.

     Validates that the task is in `input_required` status, persists the human
    decision to SQLite, emits `RuntimeEvent::TaskResumed` on the EventBus,
    and rebuilds the enriched `AIPTask` for the ORIA relaunch.

    ## HTTP codes
    - `200 OK`, resume recorded successfully
    - `404 Not Found`, task unknown to the HITL system
    - `409 Conflict`, task known but not in `input_required` status
    - `503 Service Unavailable`, HITL not configured (`task_repository` absent)
    - `500 Internal Server Error`, SQLite or internal error

    Args:
        id (str):
        body (ResumeRequest): Request body for `POST /api/v1/tasks/{id}/resume`.

            The operator submits a decision (`approved`) and an optional reason.
            The `approved` field is mandatory; omitting it produces HTTP 422.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ResumeResponse
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            body=body,
        )
    ).parsed
