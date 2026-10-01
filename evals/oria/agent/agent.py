"""Fixture agent for the ORIA agent-loop suite.

`apollia-os eval run` submits a task to an agent, and only an agent declared
orchestrated takes the ORIA path (planning, plan execution, verification). The
suite also needs the native tools, and an agent may call only the tools its
manifest declares. No shipped agent is both, so without this fixture the suite
has nothing to drive on that path.

The class carries metadata and a planning prompt, nothing else: ORIA runs the
loop, so what the suite measures is the engine, not code written here.
"""

from apollia import agent, orchestrated

PLANNING_PROMPT = """You are a measurement probe for an agent loop. Plan the \
smallest sequence of steps that answers the instruction with the tools you are \
given: read, search, edit or run what the instruction names, never answer a \
value from memory. The last step is a reasoning step (tool_hint "llm") whose \
output is exactly the answer line or lines the instruction asks for, in the \
requested protocol, with no preamble and no code fence. When a tool fails, \
the answer states the failure instead of a plausible result."""

# The native tools the ten tasks of the suite call between them. Declaring a
# tool does not make it reachable: the runtime registers what it can build on
# this path, and a task whose tool is absent says so on its own protocol line.
REQUIRED_TOOLS = (
    "file_read",
    "file_list",
    "file_glob",
    "file_grep",
    "file_write",
    "file_edit",
    "bash_executor",
    "python_executor",
    "web_search",
    "web_read",
    "memory_search",
    "ask_user",
)


@agent(
    name="eval-oria-probe",
    version="0.2.0-preview",
    description="Measurement probe: an orchestrated agent the ORIA agent-loop suite is played against.",
    tools_required=REQUIRED_TOOLS,
    memory_namespace="eval-oria-probe",
)
@orchestrated(system_prompt=PLANNING_PROMPT)
class EvalOriaProbe:
    """Orchestrated by ORIA; declares tools and a planning prompt only."""
