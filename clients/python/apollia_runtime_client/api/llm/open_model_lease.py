from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.lease_response import LeaseResponse
from ...models.open_lease_request import OpenLeaseRequest
from ...types import Response


def _get_kwargs(
    *,
    body: OpenLeaseRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/llm/leases",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | LeaseResponse | None:
    if response.status_code == 200:
        response_200 = LeaseResponse.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | LeaseResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: OpenLeaseRequest,
) -> Response[ApiErrorBody | LeaseResponse]:
    """Handler for `POST /api/v1/llm/leases`.

     A conversation moved to another model releases its lease on the previous
    one, which is unloaded when nothing else holds it.

    Args:
        body (OpenLeaseRequest): Request body for `POST /api/v1/llm/leases`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | LeaseResponse]
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
    body: OpenLeaseRequest,
) -> ApiErrorBody | LeaseResponse | None:
    """Handler for `POST /api/v1/llm/leases`.

     A conversation moved to another model releases its lease on the previous
    one, which is unloaded when nothing else holds it.

    Args:
        body (OpenLeaseRequest): Request body for `POST /api/v1/llm/leases`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | LeaseResponse
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: OpenLeaseRequest,
) -> Response[ApiErrorBody | LeaseResponse]:
    """Handler for `POST /api/v1/llm/leases`.

     A conversation moved to another model releases its lease on the previous
    one, which is unloaded when nothing else holds it.

    Args:
        body (OpenLeaseRequest): Request body for `POST /api/v1/llm/leases`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | LeaseResponse]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: OpenLeaseRequest,
) -> ApiErrorBody | LeaseResponse | None:
    """Handler for `POST /api/v1/llm/leases`.

     A conversation moved to another model releases its lease on the previous
    one, which is unloaded when nothing else holds it.

    Args:
        body (OpenLeaseRequest): Request body for `POST /api/v1/llm/leases`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | LeaseResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
