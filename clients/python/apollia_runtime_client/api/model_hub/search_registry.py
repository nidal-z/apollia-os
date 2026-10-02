from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    q: str | Unset = UNSET,
    format_: str | Unset = UNSET,
    limit: int | Unset = UNSET,
    sort: str | Unset = UNSET,
    fits_only: bool | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["q"] = q

    params["format"] = format_

    params["limit"] = limit

    params["sort"] = sort

    params["fits_only"] = fits_only

    params["hf_token"] = hf_token

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/llm/registry/search",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = cast(Any, None)
        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 502:
        response_502 = ApiErrorBody.from_dict(response.json())

        return response_502

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Any | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    q: str | Unset = UNSET,
    format_: str | Unset = UNSET,
    limit: int | Unset = UNSET,
    sort: str | Unset = UNSET,
    fits_only: bool | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> Response[Any | ApiErrorBody]:
    """`GET /api/v1/llm/registry/search?q=...&format=...&sort=...&fits_only=...`

     Asks each organisation of the two publisher lists, by its `author=`, and
    nothing else. Answers `503` with `search is unavailable offline` when no
    organisation can be reached, never an empty list.

    Args:
        q (str | Unset):
        format_ (str | Unset):
        limit (int | Unset):
        sort (str | Unset):
        fits_only (bool | Unset):
        hf_token (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        q=q,
        format_=format_,
        limit=limit,
        sort=sort,
        fits_only=fits_only,
        hf_token=hf_token,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    q: str | Unset = UNSET,
    format_: str | Unset = UNSET,
    limit: int | Unset = UNSET,
    sort: str | Unset = UNSET,
    fits_only: bool | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> Any | ApiErrorBody | None:
    """`GET /api/v1/llm/registry/search?q=...&format=...&sort=...&fits_only=...`

     Asks each organisation of the two publisher lists, by its `author=`, and
    nothing else. Answers `503` with `search is unavailable offline` when no
    organisation can be reached, never an empty list.

    Args:
        q (str | Unset):
        format_ (str | Unset):
        limit (int | Unset):
        sort (str | Unset):
        fits_only (bool | Unset):
        hf_token (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        q=q,
        format_=format_,
        limit=limit,
        sort=sort,
        fits_only=fits_only,
        hf_token=hf_token,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    q: str | Unset = UNSET,
    format_: str | Unset = UNSET,
    limit: int | Unset = UNSET,
    sort: str | Unset = UNSET,
    fits_only: bool | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> Response[Any | ApiErrorBody]:
    """`GET /api/v1/llm/registry/search?q=...&format=...&sort=...&fits_only=...`

     Asks each organisation of the two publisher lists, by its `author=`, and
    nothing else. Answers `503` with `search is unavailable offline` when no
    organisation can be reached, never an empty list.

    Args:
        q (str | Unset):
        format_ (str | Unset):
        limit (int | Unset):
        sort (str | Unset):
        fits_only (bool | Unset):
        hf_token (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        q=q,
        format_=format_,
        limit=limit,
        sort=sort,
        fits_only=fits_only,
        hf_token=hf_token,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    q: str | Unset = UNSET,
    format_: str | Unset = UNSET,
    limit: int | Unset = UNSET,
    sort: str | Unset = UNSET,
    fits_only: bool | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> Any | ApiErrorBody | None:
    """`GET /api/v1/llm/registry/search?q=...&format=...&sort=...&fits_only=...`

     Asks each organisation of the two publisher lists, by its `author=`, and
    nothing else. Answers `503` with `search is unavailable offline` when no
    organisation can be reached, never an empty list.

    Args:
        q (str | Unset):
        format_ (str | Unset):
        limit (int | Unset):
        sort (str | Unset):
        fits_only (bool | Unset):
        hf_token (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            q=q,
            format_=format_,
            limit=limit,
            sort=sort,
            fits_only=fits_only,
            hf_token=hf_token,
        )
    ).parsed
