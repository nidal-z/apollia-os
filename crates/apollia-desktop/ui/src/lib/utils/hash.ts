/**
 * Browser-side SHA-256 helper.
 *
 * Used to compute a stable fingerprint of the disclaimer checklist so that any
 * change to the wording or order forces users to re-accept the disclaimer.
 */
export async function sha256Hex(input: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(input);
  const digest = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
