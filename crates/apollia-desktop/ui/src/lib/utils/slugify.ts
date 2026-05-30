/**
 * Convert a free-form label into a channel-id slug.
 *
 * Steps:
 * 1. NFD-normalize and strip combining marks U+0300..U+036F (à → a, é → e).
 * 2. Lowercase.
 * 3. Replace any run of non-[a-z0-9] characters with a single `-`.
 * 4. Collapse repeated `-`.
 * 5. Trim leading and trailing `-`.
 *
 * Matches the backend `channel_id` regex:
 *   `^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$`
 * Returns an empty string if no usable character remains.
 */
export function slugify(input: string): string {
  if (!input) return "";

  const ascii = input
    .normalize("NFD")
    .replaceAll(/[̀-ͯ]/g, "");

  return ascii
    .toLowerCase()
    .replaceAll(/[^a-z0-9]+/g, "-")
    .replaceAll(/-+/g, "-")
    .replaceAll(/^-|-$/g, "");
}
