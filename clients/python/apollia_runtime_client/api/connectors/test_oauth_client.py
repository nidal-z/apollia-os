from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.oauth_client_test_result import OauthClientTestResult
from ...types import Response


def _get_kwargs(
    provider: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/oauth/clients/{provider}/test".format(
            provider=quote(str(provider), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | OauthClientTestResult | None:
    if response.status_code == 200:
        response_200 = OauthClientTestResult.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | OauthClientTestResult]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    provider: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | OauthClientTestResult]:
    """`POST /api/v1/oauth/clients/{provider}/test`: checks the client without a
    consent: present, well-formed, and its authorization server reachable.

    Args:
        provider (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | OauthClientTestResult]
    """

    kwargs = _get_kwargs(
        provider=provider,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    provider: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | OauthClientTestResult | None:
    """`POST /api/v1/oauth/clients/{provider}/test`: checks the client without a
    consent: present, well-formed, and its authorization server reachable.

    Args:
        provider (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | OauthClientTestResult
    """

    return sync_detailed(
        provider=provider,
        client=client,
    ).parsed


async def asyncio_detailed(
    provider: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | OauthClientTestResult]:
    """`POST /api/v1/oauth/clients/{provider}/test`: checks the client without a
    consent: present, well-formed, and its authorization server reachable.

    Args:
        provider (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | OauthClientTestResult]
    """

    kwargs = _get_kwargs(
        provider=provider,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    provider: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | OauthClientTestResult | None:
    """`POST /api/v1/oauth/clients/{provider}/test`: checks the client without a
    consent: present, well-formed, and its authorization server reachable.

    Args:
        provider (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | OauthClientTestResult
    """

    return (
        await asyncio_detailed(
            provider=provider,
            client=client,
        )
    ).parsed
