/**
 * The address a webhook trigger listens on: the native API of this
 * installation, on the port it was resolved to at startup (`[api].port`, or
 * the port a serving daemon recorded), never a port written here.
 */
export function webhookEndpoint(apiPort: number, triggerId: string): string {
  return `http://localhost:${apiPort}/api/v1/webhooks/${triggerId}`;
}
