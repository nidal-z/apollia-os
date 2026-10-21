from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.connection_activity_response import ConnectionActivityResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    tool_prefix: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["tool_prefix"] = tool_prefix

    params["since"] = since

    params["until"] = until

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/audit/activity",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ConnectionActivityResponse | None:
    if response.status_code == 200:
        response_200 = ConnectionActivityResponse.from_dict(response.json())

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
) -> Response[ApiErrorBody | ConnectionActivityResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    tool_prefix: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> Response[ApiErrorBody | ConnectionActivityResponse]:
    """`GET /api/v1/audit/activity`: the last use of the tools of a connection
    and the number of actions each agent took with them over a period.

     An action is a tool call of the connection that ended, successfully or
    not. The journal is read whole for the period, without an index. 400 when
    the prefix is missing or a bound cannot be read, 503 when the journal is
    not configured.

    Args:
        tool_prefix (str | Unset):
        since (str | Unset):
        until (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ConnectionActivityResponse]
    """

    kwargs = _get_kwargs(
        tool_prefix=tool_prefix,
        since=since,
        until=until,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    tool_prefix: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> ApiErrorBody | ConnectionActivityResponse | None:
    """`GET /api/v1/audit/activity`: the last use of the tools of a connection
    and the number of actions each agent took with them over a period.

     An action is a tool call of the connection that ended, successfully or
    not. The journal is read whole for the period, without an index. 400 when
    the prefix is missing or a bound cannot be read, 503 when the journal is
    not configured.

    Args:
        tool_prefix (str | Unset):
        since (str | Unset):
        until (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ConnectionActivityResponse
    """

    return sync_detailed(
        client=client,
        tool_prefix=tool_prefix,
        since=since,
        until=until,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    tool_prefix: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> Response[ApiErrorBody | ConnectionActivityResponse]:
    """`GET /api/v1/audit/activity`: the last use of the tools of a connection
    and the number of actions each agent took with them over a period.

     An action is a tool call of the connection that ended, successfully or
    not. The journal is read whole for the period, without an index. 400 when
    the prefix is missing or a bound cannot be read, 503 when the journal is
    not configured.

    Args:
        tool_prefix (str | Unset):
        since (str | Unset):
        until (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ConnectionActivityResponse]
    """

    kwargs = _get_kwargs(
        tool_prefix=tool_prefix,
        since=since,
        until=until,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    tool_prefix: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> ApiErrorBody | ConnectionActivityResponse | None:
    """`GET /api/v1/audit/activity`: the last use of the tools of a connection
    and the number of actions each agent took with them over a period.

     An action is a tool call of the connection that ended, successfully or
    not. The journal is read whole for the period, without an index. 400 when
    the prefix is missing or a bound cannot be read, 503 when the journal is
    not configured.

    Args:
        tool_prefix (str | Unset):
        since (str | Unset):
        until (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ConnectionActivityResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            tool_prefix=tool_prefix,
            since=since,
            until=until,
        )
    ).parsed
