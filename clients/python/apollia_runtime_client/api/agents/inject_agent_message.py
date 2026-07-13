from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.inject_message_body import InjectMessageBody
from ...models.inject_message_response import InjectMessageResponse
from ...types import Response


def _get_kwargs(
    name: str,
    *,
    body: InjectMessageBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/agents/{name}/messages".format(
            name=quote(str(name), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | InjectMessageResponse | None:
    if response.status_code == 201:
        response_201 = InjectMessageResponse.from_dict(response.json())

        return response_201

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 413:
        response_413 = ApiErrorBody.from_dict(response.json())

        return response_413

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | InjectMessageResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    body: InjectMessageBody,
) -> Response[ApiErrorBody | InjectMessageResponse]:
    """`POST /api/v1/agents/:name/messages`, inject a message from the host.

     The host deposits a message into an agent's durable inbox. The recipient is
    validated against the registry (`404` if unknown). A synthetic host-scoped
    `run_id` is allocated so the injected message is journaled like any other.
    Returns `503` when the mailbox is not available.

    Args:
        name (str):
        body (InjectMessageBody): Request body for `POST /api/v1/agents/:name/messages`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | InjectMessageResponse]
    """

    kwargs = _get_kwargs(
        name=name,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    body: InjectMessageBody,
) -> ApiErrorBody | InjectMessageResponse | None:
    """`POST /api/v1/agents/:name/messages`, inject a message from the host.

     The host deposits a message into an agent's durable inbox. The recipient is
    validated against the registry (`404` if unknown). A synthetic host-scoped
    `run_id` is allocated so the injected message is journaled like any other.
    Returns `503` when the mailbox is not available.

    Args:
        name (str):
        body (InjectMessageBody): Request body for `POST /api/v1/agents/:name/messages`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | InjectMessageResponse
    """

    return sync_detailed(
        name=name,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    body: InjectMessageBody,
) -> Response[ApiErrorBody | InjectMessageResponse]:
    """`POST /api/v1/agents/:name/messages`, inject a message from the host.

     The host deposits a message into an agent's durable inbox. The recipient is
    validated against the registry (`404` if unknown). A synthetic host-scoped
    `run_id` is allocated so the injected message is journaled like any other.
    Returns `503` when the mailbox is not available.

    Args:
        name (str):
        body (InjectMessageBody): Request body for `POST /api/v1/agents/:name/messages`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | InjectMessageResponse]
    """

    kwargs = _get_kwargs(
        name=name,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    *,
    client: AuthenticatedClient | Client,
    body: InjectMessageBody,
) -> ApiErrorBody | InjectMessageResponse | None:
    """`POST /api/v1/agents/:name/messages`, inject a message from the host.

     The host deposits a message into an agent's durable inbox. The recipient is
    validated against the registry (`404` if unknown). A synthetic host-scoped
    `run_id` is allocated so the injected message is journaled like any other.
    Returns `503` when the mailbox is not available.

    Args:
        name (str):
        body (InjectMessageBody): Request body for `POST /api/v1/agents/:name/messages`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | InjectMessageResponse
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
            body=body,
        )
    ).parsed
