from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.workflow_error_response import WorkflowErrorResponse
from ...models.workflow_view import WorkflowView
from ...types import Response


def _get_kwargs(
    package: str,
    workflow: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/workflows/{package}/{workflow}".format(
            package=quote(str(package), safe=""),
            workflow=quote(str(workflow), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> WorkflowErrorResponse | WorkflowView | None:
    if response.status_code == 200:
        response_200 = WorkflowView.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = WorkflowErrorResponse.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = WorkflowErrorResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[WorkflowErrorResponse | WorkflowView]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[WorkflowErrorResponse | WorkflowView]:
    """`GET /api/v1/workflows/{package}/{workflow}`: a workflow, its steps and
    the fields of its inputs.

    Args:
        package (str):
        workflow (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[WorkflowErrorResponse | WorkflowView]
    """

    kwargs = _get_kwargs(
        package=package,
        workflow=workflow,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
) -> WorkflowErrorResponse | WorkflowView | None:
    """`GET /api/v1/workflows/{package}/{workflow}`: a workflow, its steps and
    the fields of its inputs.

    Args:
        package (str):
        workflow (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        WorkflowErrorResponse | WorkflowView
    """

    return sync_detailed(
        package=package,
        workflow=workflow,
        client=client,
    ).parsed


async def asyncio_detailed(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[WorkflowErrorResponse | WorkflowView]:
    """`GET /api/v1/workflows/{package}/{workflow}`: a workflow, its steps and
    the fields of its inputs.

    Args:
        package (str):
        workflow (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[WorkflowErrorResponse | WorkflowView]
    """

    kwargs = _get_kwargs(
        package=package,
        workflow=workflow,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
) -> WorkflowErrorResponse | WorkflowView | None:
    """`GET /api/v1/workflows/{package}/{workflow}`: a workflow, its steps and
    the fields of its inputs.

    Args:
        package (str):
        workflow (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        WorkflowErrorResponse | WorkflowView
    """

    return (
        await asyncio_detailed(
            package=package,
            workflow=workflow,
            client=client,
        )
    ).parsed
