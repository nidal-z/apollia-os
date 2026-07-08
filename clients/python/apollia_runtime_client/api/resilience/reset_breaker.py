from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.reset_response import ResetResponse
from ...types import Response


def _get_kwargs(
    tool: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/resilience/reset/{tool}".format(
            tool=quote(str(tool), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ResetResponse | None:
    if response.status_code == 200:
        response_200 = ResetResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ResetResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    tool: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | ResetResponse]:
    """`POST /api/v1/resilience/reset/:tool`, force-close a breaker.

    Args:
        tool (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ResetResponse]
    """

    kwargs = _get_kwargs(
        tool=tool,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    tool: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | ResetResponse | None:
    """`POST /api/v1/resilience/reset/:tool`, force-close a breaker.

    Args:
        tool (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ResetResponse
    """

    return sync_detailed(
        tool=tool,
        client=client,
    ).parsed


async def asyncio_detailed(
    tool: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | ResetResponse]:
    """`POST /api/v1/resilience/reset/:tool`, force-close a breaker.

    Args:
        tool (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ResetResponse]
    """

    kwargs = _get_kwargs(
        tool=tool,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    tool: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | ResetResponse | None:
    """`POST /api/v1/resilience/reset/:tool`, force-close a breaker.

    Args:
        tool (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ResetResponse
    """

    return (
        await asyncio_detailed(
            tool=tool,
            client=client,
        )
    ).parsed
