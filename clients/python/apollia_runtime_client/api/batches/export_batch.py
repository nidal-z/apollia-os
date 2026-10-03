from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.batch_error_response import BatchErrorResponse
from ...models.export_format import ExportFormat
from ...types import UNSET, Response


def _get_kwargs(
    id: str,
    *,
    format_: ExportFormat,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    json_format_ = format_.value
    params["format"] = json_format_

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/batches/{id}/export".format(
            id=quote(str(id), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> BatchErrorResponse | str | None:
    if response.status_code == 200:
        response_200 = response.text
        return response_200

    if response.status_code == 400:
        response_400 = BatchErrorResponse.from_dict(response.json())

        return response_400

    if response.status_code == 404:
        response_404 = BatchErrorResponse.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = BatchErrorResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[BatchErrorResponse | str]:
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
    format_: ExportFormat,
) -> Response[BatchErrorResponse | str]:
    """`GET /api/v1/batches/{id}/export`: the result of a batch as a file.

     One row per item, in source order: the input columns of the item, then
    its output, its status and the sources its answer cites. `xlsx` is a
    workbook of one sheet, `csv` is UTF-8 with one header row, `json` is one
    document with the batch and an object per item. The file is offered as
    `batch-<id>.<format>`. A batch still running exports the items as they
    stand.

    Args:
        id (str):
        format_ (ExportFormat): The format a batch is exported in.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BatchErrorResponse | str]
    """

    kwargs = _get_kwargs(
        id=id,
        format_=format_,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    format_: ExportFormat,
) -> BatchErrorResponse | str | None:
    """`GET /api/v1/batches/{id}/export`: the result of a batch as a file.

     One row per item, in source order: the input columns of the item, then
    its output, its status and the sources its answer cites. `xlsx` is a
    workbook of one sheet, `csv` is UTF-8 with one header row, `json` is one
    document with the batch and an object per item. The file is offered as
    `batch-<id>.<format>`. A batch still running exports the items as they
    stand.

    Args:
        id (str):
        format_ (ExportFormat): The format a batch is exported in.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BatchErrorResponse | str
    """

    return sync_detailed(
        id=id,
        client=client,
        format_=format_,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    format_: ExportFormat,
) -> Response[BatchErrorResponse | str]:
    """`GET /api/v1/batches/{id}/export`: the result of a batch as a file.

     One row per item, in source order: the input columns of the item, then
    its output, its status and the sources its answer cites. `xlsx` is a
    workbook of one sheet, `csv` is UTF-8 with one header row, `json` is one
    document with the batch and an object per item. The file is offered as
    `batch-<id>.<format>`. A batch still running exports the items as they
    stand.

    Args:
        id (str):
        format_ (ExportFormat): The format a batch is exported in.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BatchErrorResponse | str]
    """

    kwargs = _get_kwargs(
        id=id,
        format_=format_,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    format_: ExportFormat,
) -> BatchErrorResponse | str | None:
    """`GET /api/v1/batches/{id}/export`: the result of a batch as a file.

     One row per item, in source order: the input columns of the item, then
    its output, its status and the sources its answer cites. `xlsx` is a
    workbook of one sheet, `csv` is UTF-8 with one header row, `json` is one
    document with the batch and an object per item. The file is offered as
    `batch-<id>.<format>`. A batch still running exports the items as they
    stand.

    Args:
        id (str):
        format_ (ExportFormat): The format a batch is exported in.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BatchErrorResponse | str
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            format_=format_,
        )
    ).parsed
