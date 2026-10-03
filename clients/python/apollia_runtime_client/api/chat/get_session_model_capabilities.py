from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.session_model_capabilities import SessionModelCapabilities
from ...types import Response


def _get_kwargs(
    id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/sessions/{id}/model-capabilities".format(
            id=quote(str(id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | SessionModelCapabilities | None:
    if response.status_code == 200:
        response_200 = SessionModelCapabilities.from_dict(response.json())

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
) -> Response[ApiErrorBody | SessionModelCapabilities]:
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
) -> Response[ApiErrorBody | SessionModelCapabilities]:
    """Handler for `GET /api/v1/sessions/{id}/model-capabilities`, what the
    model of a session can do.

     The model is the one the next message of the session would run on: the
    model of its declarative agent, else the backend the session prefers, else
    the default model of the machine.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | SessionModelCapabilities]
    """

    kwargs = _get_kwargs(
        id=id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | SessionModelCapabilities | None:
    """Handler for `GET /api/v1/sessions/{id}/model-capabilities`, what the
    model of a session can do.

     The model is the one the next message of the session would run on: the
    model of its declarative agent, else the backend the session prefers, else
    the default model of the machine.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | SessionModelCapabilities
    """

    return sync_detailed(
        id=id,
        client=client,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | SessionModelCapabilities]:
    """Handler for `GET /api/v1/sessions/{id}/model-capabilities`, what the
    model of a session can do.

     The model is the one the next message of the session would run on: the
    model of its declarative agent, else the backend the session prefers, else
    the default model of the machine.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | SessionModelCapabilities]
    """

    kwargs = _get_kwargs(
        id=id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | SessionModelCapabilities | None:
    """Handler for `GET /api/v1/sessions/{id}/model-capabilities`, what the
    model of a session can do.

     The model is the one the next message of the session would run on: the
    model of its declarative agent, else the backend the session prefers, else
    the default model of the machine.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | SessionModelCapabilities
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
        )
    ).parsed
