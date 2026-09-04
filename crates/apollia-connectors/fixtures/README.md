# Connector replay fixtures

One JSON file per connector operation. Each holds an upstream answer and the
reading the connector must produce from it. The replay harness lives in
`../src/replay/`; it serves the answer from a local mock server, calls the real
client method, and compares.

**The files here today are hand-written examples, not recordings.** They were
typed from the vendors' public reference pages to prove the harness runs end to
end. They say nothing about what Google or Microsoft actually return. Every one
of them declares `"origin": "example"`, and the guard counts examples and
captures separately for exactly that reason.

## Recording one

You need a throwaway account on the provider, and the operation's HTTP answer.
Any of these gives it to you:

- `curl` the endpoint with a bearer token from the account, and keep the body.
- Read the answer off the wire with a local proxy.
- Copy the response pane from the vendor's own API explorer.

Then create `<operation_id>.json` and paste the body into `response`, unchanged.
Do not pretty-print it, trim it, or drop fields you think are noise: the fields
you drop are the ones the harness would otherwise catch a decoder mishandling.

```json
{
  "operation": "gmail.send",
  "origin": "capture",
  "note": "throwaway account, 2026-09-04, users.messages.send",
  "response": { "id": "18f0", "threadId": "18f0", "labelIds": ["SENT"] },
  "expect": { "id": "18f0", "threadId": "18f0" }
}
```

`expect` is the value the client method returns, serialised. Run the suite once
with a placeholder and the failure prints what was actually read, which is the
line to paste back in. That is the intended way to fill it: the harness tells
you the answer, and you check it is the right one.

## Fields

| Field | Required | Meaning |
|---|---|---|
| `operation` | yes | Operation id, e.g. `gmail.send`. Must match the file name. |
| `origin` | yes | `capture` for a real recording, `example` for a hand-written one. |
| `note` | yes | Provenance. Which account, which date, which reference page. |
| `status` | no | HTTP status served, default `200`. `429` and `5xx` are refused. |
| `response` | one of | The body, verbatim, for an operation that makes one call. |
| `responses` | one of | Bodies in call order, for an operation that makes several. |
| `expect` | one of | The reading the client must produce. |
| `expect_error` | one of | Substring the surfaced error must contain, for a failure answer. |

A JSON string in `response` is served as a raw body rather than as JSON, which
is how `gdrive.read_file` and `onedrive.download` are recorded.

## Several fixtures for one operation

Add a suffix: `gsheets.read_values.json` and `gsheets.read_values.error.json`
both cover `gsheets.read_values`. The `operation` field, not the file name,
decides which operation a file covers.

## What the harness refuses

Each of these fails the suite rather than passing quietly.

- A key it does not know, so a misspelling is not silently ignored.
- A file name that does not match the operation it declares.
- An empty `note`.
- Both `response` and `responses`, or neither. Same for `expect` and
  `expect_error`.
- A `429` or `5xx` status: those drive the retry ladder, which sleeps for
  seconds, and are already covered by the tests in `../src/http.rs`.
- A fixture that describes fewer upstream calls than the operation makes. Drive
  resolves a folder before it lists it, so `gdrive.workspace_list` needs
  `responses`, not `response`.
- An empty directory. A harness with nothing to replay measured nothing, and
  reports that rather than a pass.

## Coverage

`python3 scripts/check_connector_fixtures.py` counts the operations with no
fixture and holds them on a descending ratchet.
