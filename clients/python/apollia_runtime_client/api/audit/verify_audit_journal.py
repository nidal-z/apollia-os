from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.verify_journal_report import VerifyJournalReport
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/audit/verify",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | VerifyJournalReport | None:
    if response.status_code == 200:
        response_200 = VerifyJournalReport.from_dict(response.json())

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
) -> Response[ApiErrorBody | VerifyJournalReport]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | VerifyJournalReport]:
    """`GET /api/v1/audit/verify`, verify the whole journal across all runs.

     Unlike the per-run route, this walks the global chain (detecting interior
    deletion and whole-run deletion) and compares the terminal head to the
    persisted anchor (detecting global-tail truncation). An empty journal is a
    valid 200 `ok:true`, not a 404. Returns 503 when the journal is not
    configured and 500 on an internal error.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | VerifyJournalReport]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | VerifyJournalReport | None:
    """`GET /api/v1/audit/verify`, verify the whole journal across all runs.

     Unlike the per-run route, this walks the global chain (detecting interior
    deletion and whole-run deletion) and compares the terminal head to the
    persisted anchor (detecting global-tail truncation). An empty journal is a
    valid 200 `ok:true`, not a 404. Returns 503 when the journal is not
    configured and 500 on an internal error.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | VerifyJournalReport
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | VerifyJournalReport]:
    """`GET /api/v1/audit/verify`, verify the whole journal across all runs.

     Unlike the per-run route, this walks the global chain (detecting interior
    deletion and whole-run deletion) and compares the terminal head to the
    persisted anchor (detecting global-tail truncation). An empty journal is a
    valid 200 `ok:true`, not a 404. Returns 503 when the journal is not
    configured and 500 on an internal error.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | VerifyJournalReport]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | VerifyJournalReport | None:
    """`GET /api/v1/audit/verify`, verify the whole journal across all runs.

     Unlike the per-run route, this walks the global chain (detecting interior
    deletion and whole-run deletion) and compares the terminal head to the
    persisted anchor (detecting global-tail truncation). An empty journal is a
    valid 200 `ok:true`, not a 404. Returns 503 when the journal is not
    configured and 500 on an internal error.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | VerifyJournalReport
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
