from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.audit_journal_entry_detail_response import AuditJournalEntryDetailResponse
from ...types import Response


def _get_kwargs(
    run_id: str,
    seq: int,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/audit/journal/{run_id}/entries/{seq}".format(
            run_id=quote(str(run_id), safe=""),
            seq=quote(str(seq), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | AuditJournalEntryDetailResponse | None:
    if response.status_code == 200:
        response_200 = AuditJournalEntryDetailResponse.from_dict(response.json())

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
) -> Response[ApiErrorBody | AuditJournalEntryDetailResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    run_id: str,
    seq: int,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | AuditJournalEntryDetailResponse]:
    """`GET /api/v1/audit/journal/:run_id/entries/:seq`, one entry with its detail.

     The page gives each entry a summary without content. This gives the entry
    whole and, for a tool call, the full arguments and the full output of that
    call, joined by the run and the call identifier from the entries the run
    captured for replay. 404 when the run has no such entry, 503 when the
    journal is not configured.

    Args:
        run_id (str):
        seq (int):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | AuditJournalEntryDetailResponse]
    """

    kwargs = _get_kwargs(
        run_id=run_id,
        seq=seq,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    run_id: str,
    seq: int,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | AuditJournalEntryDetailResponse | None:
    """`GET /api/v1/audit/journal/:run_id/entries/:seq`, one entry with its detail.

     The page gives each entry a summary without content. This gives the entry
    whole and, for a tool call, the full arguments and the full output of that
    call, joined by the run and the call identifier from the entries the run
    captured for replay. 404 when the run has no such entry, 503 when the
    journal is not configured.

    Args:
        run_id (str):
        seq (int):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | AuditJournalEntryDetailResponse
    """

    return sync_detailed(
        run_id=run_id,
        seq=seq,
        client=client,
    ).parsed


async def asyncio_detailed(
    run_id: str,
    seq: int,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | AuditJournalEntryDetailResponse]:
    """`GET /api/v1/audit/journal/:run_id/entries/:seq`, one entry with its detail.

     The page gives each entry a summary without content. This gives the entry
    whole and, for a tool call, the full arguments and the full output of that
    call, joined by the run and the call identifier from the entries the run
    captured for replay. 404 when the run has no such entry, 503 when the
    journal is not configured.

    Args:
        run_id (str):
        seq (int):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | AuditJournalEntryDetailResponse]
    """

    kwargs = _get_kwargs(
        run_id=run_id,
        seq=seq,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    run_id: str,
    seq: int,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | AuditJournalEntryDetailResponse | None:
    """`GET /api/v1/audit/journal/:run_id/entries/:seq`, one entry with its detail.

     The page gives each entry a summary without content. This gives the entry
    whole and, for a tool call, the full arguments and the full output of that
    call, joined by the run and the call identifier from the entries the run
    captured for replay. 404 when the run has no such entry, 503 when the
    journal is not configured.

    Args:
        run_id (str):
        seq (int):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | AuditJournalEntryDetailResponse
    """

    return (
        await asyncio_detailed(
            run_id=run_id,
            seq=seq,
            client=client,
        )
    ).parsed
