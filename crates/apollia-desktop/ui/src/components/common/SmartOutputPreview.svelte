<!--
  Compact 1-line preview of a task output.

  Extracts the "summary" (or "executive_summary" / "description") field from
  JSON output and truncates it to MAX_PREVIEW_LENGTH characters. Falls back to
  the first line of raw text for non-JSON output.

  Smart output renderer — preview variant
-->
<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    /** Raw output string (JSON or plain text). */
    output: string;
    /** Maximum characters before truncation. */
    maxLength?: number;
    /** Optional class override. */
    class?: string;
  }

  const MAX_PREVIEW_LENGTH = 100;

  let { output, maxLength = MAX_PREVIEW_LENGTH, class: className = "" }: Props = $props();

  /** Extract a summary line from the output. */
  const preview = $derived(extractPreview(output, maxLength));

  /** Summary field names in priority order. */
  const SUMMARY_FIELDS = ["summary", "executive_summary", "description"] as const;

  function extractPreview(raw: string, limit: number): string {
    try {
      const data: unknown = JSON.parse(raw);
      if (typeof data === "object" && data !== null && !Array.isArray(data)) {
        const obj = data as Record<string, unknown>;
        for (const field of SUMMARY_FIELDS) {
          if (typeof obj[field] === "string" && obj[field]) {
            return truncate(obj[field] as string, limit);
          }
        }
        if (typeof obj["title"] === "string" && obj["title"]) {
          return truncate(obj["title"] as string, limit);
        }
      }
    } catch {
      // Not JSON — fall through to plain text handling
    }

    const firstLine = raw.split("\n")[0] ?? raw;
    return truncate(firstLine, limit);
  }

  function truncate(str: string, limit: number): string {
    if (str.length <= limit) return str;
    return str.slice(0, limit) + "…";
  }
</script>

<span
  class={cn("truncate text-sm text-muted-foreground", className)}
  title={output}
  data-testid="smart-output-preview"
>
  {preview}
</span>
