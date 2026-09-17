---
sidebar_position: 6.7
title: Ask the model for structured output
---

# Ask the model for structured output

An agent that needs a value, not a paragraph, passes a JSON Schema to
`ctx.llm.complete` or `ctx.llm.chat`. The call then constrains what the model may
emit, checks what came back against the same schema, and hands you the validated
value as a plain Python object. There is nothing to parse and no regular
expression to maintain.

## Pass a schema, receive a value

```python
from apollia import agent, skill

REPORT = {
    "type": "object",
    "properties": {
        "title": {"type": "string"},
        "count": {"type": "integer"},
    },
    "required": ["title", "count"],
}


@agent(name="reporter", version="1.0.0", description="Summarizes into a record")
class Reporter:
    @skill(id="summarize")
    async def summarize(self, payload, ctx):
        report = await ctx.llm.complete(
            [{"role": "user", "content": payload["text"]}],
            schema=REPORT,
        )
        # `report` is a dict, already checked against REPORT.
        return {"title": report["title"], "count": report["count"]}
```

Without `schema` the call returns an `LlmResponse` and `response.content` is a
string, as before. Adding `schema` changes the return type, which is the point:
a call that promises a shape should not hand back something you still have to
inspect.

## What the schema may contain

<!-- claim:llm-schema-constrains-local-generation -->
Objects and arrays nest freely, and their members are typed. On the embedded
`llama-server` the schema becomes a GBNF grammar that constrains decoding token
by token, so the model cannot emit a shape the schema forbids. On any other
OpenAI-compatible provider it travels as `response_format`, the structured
output surface that protocol defines.

| Construct | Supported |
| --- | --- |
| `object` with `properties`, nested | yes |
| `array` with `items`, nested | yes |
| `string`, `number`, `integer`, `boolean`, `null` | yes |
| `enum`, on any type | yes |
| `required` | yes |
| `minimum`, `maximum`, `minLength`, `maxLength`, `minItems`, `maxItems` | checked, not constrained |
| `anyOf`, `oneOf`, `allOf`, `$ref` | refused by name |

The last two rows are the interesting ones.

A bound on a number or a length is checked after generation and never during it,
because no decoding grammar expresses one. Declare them: they are enforced, just
one step later than the rest.

A combinator is refused rather than ignored. An unsupported construct that
quietly relaxed the constraint would leave you validating an answer nobody
constrained, and blaming the model for it.

## When it fails

A failure raises `apollia.errors.StructuredOutputError`, which carries the JSON
path of the offending node so you branch on a field rather than on a sentence.

```python
from apollia.errors import StructuredOutputError

try:
    report = await ctx.llm.complete(messages, schema=REPORT)
except StructuredOutputError as e:
    ctx.logger.warning("structured output failed at %s: %s", e.path, e.reason)
```

`e.kind` tells the three cases apart:

- `schema_unsupported`: the schema cannot become a constraint. Raised before the
  model is called, so no tokens were spent.
- `backend_unsupported`: the resolved backend has no structured output mode.
  Anthropic and Vertex are in this case today.
- `response_invalid`: the answer came back and the schema refuses it.

Nothing is retried for you. Whether a second attempt is worth its tokens depends
on the agent, and the runtime does not get to decide that.

## What it costs

A constrained call is one call to the model, so it charges one step against the
agent's `StepBudget`, exactly like an unconstrained one. The audit trail records
that generation was constrained and a fingerprint of the schema; the schema
itself is never written to the journal.

## Next steps

- [`ctx.llm` reference](/reference/sdk/llm)
- [Write a worker](/how-to/write-a-worker)
