from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.turn_llm_calls_response import TurnLlmCallsResponse
from ...types import Response


def _get_kwargs(
    id: str,
    turn_id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/sessions/{id}/turns/{turn_id}/llm-calls".format(
            id=quote(str(id), safe=""),
            turn_id=quote(str(turn_id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | TurnLlmCallsResponse | None:
    if response.status_code == 200:
        response_200 = TurnLlmCallsResponse.from_dict(response.json())

        return response_200

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | TurnLlmCallsResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    id: str,
    turn_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | TurnLlmCallsResponse]:
    """Handler for `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`.

    Args:
        id (str):
        turn_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | TurnLlmCallsResponse]
    """

    kwargs = _get_kwargs(
        id=id,
        turn_id=turn_id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    turn_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | TurnLlmCallsResponse | None:
    """Handler for `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`.

    Args:
        id (str):
        turn_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | TurnLlmCallsResponse
    """

    return sync_detailed(
        id=id,
        turn_id=turn_id,
        client=client,
    ).parsed


async def asyncio_detailed(
    id: str,
    turn_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | TurnLlmCallsResponse]:
    """Handler for `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`.

    Args:
        id (str):
        turn_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | TurnLlmCallsResponse]
    """

    kwargs = _get_kwargs(
        id=id,
        turn_id=turn_id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    turn_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | TurnLlmCallsResponse | None:
    """Handler for `GET /api/v1/sessions/{id}/turns/{turn_id}/llm-calls`.

    Args:
        id (str):
        turn_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | TurnLlmCallsResponse
    """

    return (
        await asyncio_detailed(
            id=id,
            turn_id=turn_id,
            client=client,
        )
    ).parsed
