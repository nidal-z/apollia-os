# @apollia/runtime-client (TypeScript)

Typed client a host application uses to drive an Apollia runtime over its HTTP
API. Types are generated from the runtime's OpenAPI spec with
`openapi-typescript`; requests go through `openapi-fetch`, so every operation is
fully typed against the live contract.

## Usage

```ts
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { createApolliaClient } from "@apollia/runtime-client";

const token = readFileSync(`${homedir()}/.apollia/api-token`, "utf8").trim();
const apollia = createApolliaClient({ token }); // baseUrl defaults to 127.0.0.1:7771

const health = await apollia.GET("/api/v1/health");
console.log(health.data); // { status: "ok" }

const submit = await apollia.POST("/api/v1/tasks", {
  body: {
    agent_id: "echo",
    input: { parts: [{ type: "text", text: "hello from the host SDK" }] },
  },
});
console.log(submit.data); // { task_id, status, ... }
```

## Regenerate

```sh
npm install
npm run generate   # openapi-typescript ../openapi.json -o src/schema.ts
```

`src/schema.ts` is generated; do not hand-edit it. The committed spec is
`clients/openapi.json`; refresh it from a running daemon with
`bash clients/regen.sh --from-daemon`.
