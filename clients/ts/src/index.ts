// Typed host-driving client for the Apollia OS runtime API.
//
// Thin wrapper over openapi-fetch bound to the generated `paths` types in
// ./schema.ts. Regenerate the schema with `npm run generate` after the runtime
// contract changes; do not hand-edit schema.ts.

import createClient from "openapi-fetch";
import type { paths } from "./schema";

export interface ApolliaClientOptions {
  /** Base URL of the runtime TCP listener. Defaults to loopback:7771. */
  baseUrl?: string;
  /** Bearer token from ~/.apollia/api-token (required over TCP). */
  token: string;
}

/**
 * Create a typed client that drives an Apollia runtime over TCP + bearer token.
 *
 * Every operation in the OpenAPI contract is available with full request and
 * response typing, e.g.:
 *
 *   const apollia = createApolliaClient({ token });
 *   const { data } = await apollia.POST("/api/v1/tasks", {
 *     body: { agent_id: "echo", input: { parts: [{ type: "text", text: "hi" }] } },
 *   });
 */
export function createApolliaClient(opts: ApolliaClientOptions) {
  return createClient<paths>({
    baseUrl: opts.baseUrl ?? "http://127.0.0.1:7771",
    headers: { Authorization: `Bearer ${opts.token}` },
  });
}

export type { paths, components } from "./schema";
