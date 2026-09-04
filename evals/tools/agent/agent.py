"""Fixture agent for the native-tool eval suites.

It exists for one reason: `apollia-os eval run` submits a task to an agent,
and the tools an agent may call are the ones its manifest declares. No shipped
agent declares the native tools, so without this fixture the suites in this
directory have nothing to drive.

The handler is deliberately thin. It hands every tool the runtime granted this
agent to the SDK's ReAct loop and returns the final answer verbatim, so the
answer the harness asserts on is the model's own last line, not something this
file reformatted.
"""

from apollia import agent, on_message, react
from apollia.types import Ctx

SYSTEM_PROMPT = """You are a measurement probe. Each instruction names one \
tool, the exact target it must act on, and the exact single line your answer \
must be. Call that tool, with those arguments, and answer with that line and \
nothing else: no preamble, no explanation, no code fence. Never guess a value \
you were asked to read: if the tool fails, answer with the tool's error \
message on the requested line instead of inventing a plausible result."""

# Every native tool the runtime knows, in the order of NATIVE_TOOL_NAMES
# (crates/apollia-tools/src/tool_registry.rs). Declaring a tool here does not
# make it reachable: the runtime registers what it can build, and the eval
# path registers neither ask_user nor an http_fetch allowlist. That gap is the
# measurement, and it is written down in this directory's README.
REQUIRED_TOOLS = (
    "bash_executor",
    "python_executor",
    "file_read",
    "file_write",
    "file_list",
    "file_edit",
    "file_glob",
    "file_grep",
    "notebook_read",
    "notebook_edit",
    "http_fetch",
    "web_search",
    "web_read",
    "memory_search",
    "ask_user",
    "permission_rule_add",
    "permission_rule_remove",
    "permission_rule_list",
)


@agent(
    name="eval-tools-probe",
    version="0.1.0-preview",
    description="Measurement probe: calls one named native tool per task and reports its observable effect.",
    tools_required=REQUIRED_TOOLS,
    memory_namespace="eval-tools-probe",
)
class EvalToolsProbe:
    """Drives one named native tool per task, through a ReAct loop."""

    @on_message
    async def handle(self, message: str, history: list[dict], ctx: Ctx) -> str:
        """Expose every reachable tool to the loop and return its final answer."""
        descriptors: list[dict] = []
        for name in ctx.tools.list_tools():
            descriptor = await ctx.tools.describe(name)
            if descriptor is not None:
                descriptors.append(descriptor)
        ctx.logger.info(
            "eval-tools-probe: %d tools reachable for this run", len(descriptors)
        )
        return await react(
            ctx,
            system=SYSTEM_PROMPT,
            user=message,
            tools=descriptors,
            max_steps=8,
        )
