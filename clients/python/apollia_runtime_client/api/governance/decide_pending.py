from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.decide_request import DecideRequest
from ...models.decide_response import DecideResponse
from ...models.pending_error_body import PendingErrorBody
from ...types import Response


def _get_kwargs(
    *,
    body: DecideRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/pending-decisions/decide",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> DecideResponse | PendingErrorBody | None:
    if response.status_code == 200:
        response_200 = DecideResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = PendingErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = PendingErrorBody.from_dict(response.json())

        return response_409

    if response.status_code == 422:
        response_422 = PendingErrorBody.from_dict(response.json())

        return response_422

    if response.status_code == 503:
        response_503 = PendingErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[DecideResponse | PendingErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: DecideRequest,
) -> Response[DecideResponse | PendingErrorBody]:
    """`POST /api/v1/pending-decisions/decide`: answer one waiting decision.

     The answer goes through the call that answers the item where it was born,
    so the conversation, the task or the batch item goes on as if it had been
    answered there. After `always` on a batch item, every waiting item of the
    same agent the consent covers runs again.

    Args:
        body (DecideRequest): Body of `POST /api/v1/pending-decisions/decide`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[DecideResponse | PendingErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    body: DecideRequest,
) -> DecideResponse | PendingErrorBody | None:
    """`POST /api/v1/pending-decisions/decide`: answer one waiting decision.

     The answer goes through the call that answers the item where it was born,
    so the conversation, the task or the batch item goes on as if it had been
    answered there. After `always` on a batch item, every waiting item of the
    same agent the consent covers runs again.

    Args:
        body (DecideRequest): Body of `POST /api/v1/pending-decisions/decide`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        DecideResponse | PendingErrorBody
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: DecideRequest,
) -> Response[DecideResponse | PendingErrorBody]:
    """`POST /api/v1/pending-decisions/decide`: answer one waiting decision.

     The answer goes through the call that answers the item where it was born,
    so the conversation, the task or the batch item goes on as if it had been
    answered there. After `always` on a batch item, every waiting item of the
    same agent the consent covers runs again.

    Args:
        body (DecideRequest): Body of `POST /api/v1/pending-decisions/decide`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[DecideResponse | PendingErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: DecideRequest,
) -> DecideResponse | PendingErrorBody | None:
    """`POST /api/v1/pending-decisions/decide`: answer one waiting decision.

     The answer goes through the call that answers the item where it was born,
    so the conversation, the task or the batch item goes on as if it had been
    answered there. After `always` on a batch item, every waiting item of the
    same agent the consent covers runs again.

    Args:
        body (DecideRequest): Body of `POST /api/v1/pending-decisions/decide`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        DecideResponse | PendingErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
