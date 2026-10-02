from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.server_probe_error_response import ServerProbeErrorResponse
from ...models.test_server_request import TestServerRequest
from ...models.test_server_response import TestServerResponse
from ...types import Response


def _get_kwargs(
    *,
    body: TestServerRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/llm/servers/test",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ServerProbeErrorResponse | TestServerResponse | None:
    if response.status_code == 200:
        response_200 = TestServerResponse.from_dict(response.json())

        return response_200

    if response.status_code == 422:
        response_422 = ServerProbeErrorResponse.from_dict(response.json())

        return response_422

    if response.status_code == 502:
        response_502 = ServerProbeErrorResponse.from_dict(response.json())

        return response_502

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ServerProbeErrorResponse | TestServerResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: TestServerRequest,
) -> Response[ServerProbeErrorResponse | TestServerResponse]:
    """`POST /api/v1/llm/servers/test`: list the models of a server that is not
    saved, to check the address and the key before saving. Writes nothing.

    Args:
        body (TestServerRequest): Body of `POST /api/v1/llm/servers/test`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ServerProbeErrorResponse | TestServerResponse]
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
    body: TestServerRequest,
) -> ServerProbeErrorResponse | TestServerResponse | None:
    """`POST /api/v1/llm/servers/test`: list the models of a server that is not
    saved, to check the address and the key before saving. Writes nothing.

    Args:
        body (TestServerRequest): Body of `POST /api/v1/llm/servers/test`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ServerProbeErrorResponse | TestServerResponse
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: TestServerRequest,
) -> Response[ServerProbeErrorResponse | TestServerResponse]:
    """`POST /api/v1/llm/servers/test`: list the models of a server that is not
    saved, to check the address and the key before saving. Writes nothing.

    Args:
        body (TestServerRequest): Body of `POST /api/v1/llm/servers/test`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ServerProbeErrorResponse | TestServerResponse]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: TestServerRequest,
) -> ServerProbeErrorResponse | TestServerResponse | None:
    """`POST /api/v1/llm/servers/test`: list the models of a server that is not
    saved, to check the address and the key before saving. Writes nothing.

    Args:
        body (TestServerRequest): Body of `POST /api/v1/llm/servers/test`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ServerProbeErrorResponse | TestServerResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
