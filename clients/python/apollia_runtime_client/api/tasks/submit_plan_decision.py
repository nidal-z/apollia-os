from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.plan_decision_request import PlanDecisionRequest
from ...models.plan_decision_response import PlanDecisionResponse
from ...types import Response


def _get_kwargs(
    id: str,
    *,
    body: PlanDecisionRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/tasks/{id}/plan-decision".format(
            id=quote(str(id), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | PlanDecisionResponse | None:
    if response.status_code == 200:
        response_200 = PlanDecisionResponse.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

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
) -> Response[ApiErrorBody | PlanDecisionResponse]:
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
    body: PlanDecisionRequest,
) -> Response[ApiErrorBody | PlanDecisionResponse]:
    """Handler for `POST /api/v1/tasks/{id}/plan-decision`.

     Resolves the plan gate registered for the run, unblocking the engine that
    paused after plan generation. The path id is the run identifier (the task
    id on the orchestrated path).

    ## HTTP codes
    - `200 OK`, decision recorded and the gate resolved
    - `400 Bad Request`, unknown `decision` value
    - `404 Not Found`, no gate pending for this run (unknown, resolved, expired)
    - `503 Service Unavailable`, the plan gate is not configured

    Args:
        id (str):
        body (PlanDecisionRequest): Request body for `POST /api/v1/tasks/{id}/plan-decision`.

            The operator approves the generated plan or rejects it with optional
            feedback used to guide replanning.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | PlanDecisionResponse]
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
    body: PlanDecisionRequest,
) -> ApiErrorBody | PlanDecisionResponse | None:
    """Handler for `POST /api/v1/tasks/{id}/plan-decision`.

     Resolves the plan gate registered for the run, unblocking the engine that
    paused after plan generation. The path id is the run identifier (the task
    id on the orchestrated path).

    ## HTTP codes
    - `200 OK`, decision recorded and the gate resolved
    - `400 Bad Request`, unknown `decision` value
    - `404 Not Found`, no gate pending for this run (unknown, resolved, expired)
    - `503 Service Unavailable`, the plan gate is not configured

    Args:
        id (str):
        body (PlanDecisionRequest): Request body for `POST /api/v1/tasks/{id}/plan-decision`.

            The operator approves the generated plan or rejects it with optional
            feedback used to guide replanning.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | PlanDecisionResponse
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
    body: PlanDecisionRequest,
) -> Response[ApiErrorBody | PlanDecisionResponse]:
    """Handler for `POST /api/v1/tasks/{id}/plan-decision`.

     Resolves the plan gate registered for the run, unblocking the engine that
    paused after plan generation. The path id is the run identifier (the task
    id on the orchestrated path).

    ## HTTP codes
    - `200 OK`, decision recorded and the gate resolved
    - `400 Bad Request`, unknown `decision` value
    - `404 Not Found`, no gate pending for this run (unknown, resolved, expired)
    - `503 Service Unavailable`, the plan gate is not configured

    Args:
        id (str):
        body (PlanDecisionRequest): Request body for `POST /api/v1/tasks/{id}/plan-decision`.

            The operator approves the generated plan or rejects it with optional
            feedback used to guide replanning.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | PlanDecisionResponse]
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
    body: PlanDecisionRequest,
) -> ApiErrorBody | PlanDecisionResponse | None:
    """Handler for `POST /api/v1/tasks/{id}/plan-decision`.

     Resolves the plan gate registered for the run, unblocking the engine that
    paused after plan generation. The path id is the run identifier (the task
    id on the orchestrated path).

    ## HTTP codes
    - `200 OK`, decision recorded and the gate resolved
    - `400 Bad Request`, unknown `decision` value
    - `404 Not Found`, no gate pending for this run (unknown, resolved, expired)
    - `503 Service Unavailable`, the plan gate is not configured

    Args:
        id (str):
        body (PlanDecisionRequest): Request body for `POST /api/v1/tasks/{id}/plan-decision`.

            The operator approves the generated plan or rejects it with optional
            feedback used to guide replanning.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | PlanDecisionResponse
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            body=body,
        )
    ).parsed
