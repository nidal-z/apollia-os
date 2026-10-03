from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.applied_proposal import AppliedProposal
from ...types import Response


def _get_kwargs(
    index: int,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/permissions/proposals/{index}/apply".format(
            index=quote(str(index), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | AppliedProposal | None:
    if response.status_code == 200:
        response_200 = AppliedProposal.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 422:
        response_422 = ApiErrorBody.from_dict(response.json())

        return response_422

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | AppliedProposal]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    index: int,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | AppliedProposal]:
    """`POST /api/v1/permissions/proposals/{index}/apply`: stores the proposal as
    a rule and removes it from the list.

    Args:
        index (int):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | AppliedProposal]
    """

    kwargs = _get_kwargs(
        index=index,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    index: int,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | AppliedProposal | None:
    """`POST /api/v1/permissions/proposals/{index}/apply`: stores the proposal as
    a rule and removes it from the list.

    Args:
        index (int):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | AppliedProposal
    """

    return sync_detailed(
        index=index,
        client=client,
    ).parsed


async def asyncio_detailed(
    index: int,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | AppliedProposal]:
    """`POST /api/v1/permissions/proposals/{index}/apply`: stores the proposal as
    a rule and removes it from the list.

    Args:
        index (int):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | AppliedProposal]
    """

    kwargs = _get_kwargs(
        index=index,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    index: int,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | AppliedProposal | None:
    """`POST /api/v1/permissions/proposals/{index}/apply`: stores the proposal as
    a rule and removes it from the list.

    Args:
        index (int):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | AppliedProposal
    """

    return (
        await asyncio_detailed(
            index=index,
            client=client,
        )
    ).parsed
