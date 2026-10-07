"""Test of the echo workflow, run from the package folder.

    cd clients/examples/echo && python -m pytest tests
"""

import asyncio

from apollia.testing import MockContext
from workflows.echo import echo


def test_the_workflow_returns_its_message_without_a_model():
    # GIVEN a context whose model has no answer queued
    ctx = MockContext()

    # WHEN the workflow runs on a message
    result = asyncio.run(echo(ctx, {"message": "hello from the host SDK"}))

    # THEN the message comes back as written
    assert result == {"echo": "hello from the host SDK"}
