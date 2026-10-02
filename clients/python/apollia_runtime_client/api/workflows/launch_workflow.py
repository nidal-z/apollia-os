from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.workflow_error_response import WorkflowErrorResponse
from ...models.workflow_launch import WorkflowLaunch
from ...models.workflow_run_view import WorkflowRunView
from ...types import Response


def _get_kwargs(
    package: str,
    workflow: str,
    *,
    body: WorkflowLaunch,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/workflows/{package}/{workflow}/runs".format(
            package=quote(str(package), safe=""),
            workflow=quote(str(workflow), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> WorkflowErrorResponse | WorkflowRunView | None:
    if response.status_code == 202:
        response_202 = WorkflowRunView.from_dict(response.json())

        return response_202

    if response.status_code == 400:
        response_400 = WorkflowErrorResponse.from_dict(response.json())

        return response_400

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
) -> Response[WorkflowErrorResponse | WorkflowRunView]:
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
    body: WorkflowLaunch,
) -> Response[WorkflowErrorResponse | WorkflowRunView]:
    """`POST /api/v1/workflows/{package}/{workflow}/runs`: run a workflow.

     The inputs are checked and the run recorded, with its declared steps,
    before the answer; the run goes on in the background.

    Args:
        package (str):
        workflow (str):
        body (WorkflowLaunch): Body of `POST /api/v1/workflows/{package}/{workflow}/runs`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[WorkflowErrorResponse | WorkflowRunView]
    """

    kwargs = _get_kwargs(
        package=package,
        workflow=workflow,
        body=body,
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
    body: WorkflowLaunch,
) -> WorkflowErrorResponse | WorkflowRunView | None:
    """`POST /api/v1/workflows/{package}/{workflow}/runs`: run a workflow.

     The inputs are checked and the run recorded, with its declared steps,
    before the answer; the run goes on in the background.

    Args:
        package (str):
        workflow (str):
        body (WorkflowLaunch): Body of `POST /api/v1/workflows/{package}/{workflow}/runs`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        WorkflowErrorResponse | WorkflowRunView
    """

    return sync_detailed(
        package=package,
        workflow=workflow,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
    body: WorkflowLaunch,
) -> Response[WorkflowErrorResponse | WorkflowRunView]:
    """`POST /api/v1/workflows/{package}/{workflow}/runs`: run a workflow.

     The inputs are checked and the run recorded, with its declared steps,
    before the answer; the run goes on in the background.

    Args:
        package (str):
        workflow (str):
        body (WorkflowLaunch): Body of `POST /api/v1/workflows/{package}/{workflow}/runs`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[WorkflowErrorResponse | WorkflowRunView]
    """

    kwargs = _get_kwargs(
        package=package,
        workflow=workflow,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
    body: WorkflowLaunch,
) -> WorkflowErrorResponse | WorkflowRunView | None:
    """`POST /api/v1/workflows/{package}/{workflow}/runs`: run a workflow.

     The inputs are checked and the run recorded, with its declared steps,
    before the answer; the run goes on in the background.

    Args:
        package (str):
        workflow (str):
        body (WorkflowLaunch): Body of `POST /api/v1/workflows/{package}/{workflow}/runs`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        WorkflowErrorResponse | WorkflowRunView
    """

    return (
        await asyncio_detailed(
            package=package,
            workflow=workflow,
            client=client,
            body=body,
        )
    ).parsed
