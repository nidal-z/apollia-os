<!--
  Raw JSON viewer with basic syntax highlighting.

  Renders JSON with indentation and color-coded tokens (strings, numbers,
  booleans, null, keys). Uses simple regex-based highlighting — no external
  dependency required.

  Smart output renderer — raw JSON view
-->
<script lang="ts">
  import { cn } from "$lib/utils";

  interface Props {
    /** Raw JSON string to display. */
    json: string;
    /** Optional class override. */
    class?: string;
  }

  let { json, class: className = "" }: Props = $props();

  /** Pretty-print the JSON string with 2-space indentation. */
  const prettyJson = $derived(formatJson(json));

  function formatJson(raw: string): string {
    try {
      const parsed: unknown = JSON.parse(raw);
      return JSON.stringify(parsed, null, 2);
    } catch {
      return raw;
    }
  }

  /**
   * Apply basic syntax highlighting to a JSON string.
   * Returns an array of segments with type annotations for coloring.
   */
  type TokenType = "key" | "string" | "number" | "boolean" | "null" | "punctuation" | "plain";

  interface Token {
    type: TokenType;
    text: string;
  }

  const tokens = $derived(tokenize(prettyJson));

  function tokenize(source: string): Token[] {
    const result: Token[] = [];
    const regex = /("(?:\\.|[^"\\])*"\s*:)|("(?:\\.|[^"\\])*")|((?:-?\d+\.?\d*(?:[eE][+-]?\d+)?)(?=[,\s\]}\n]|$))|(true|false)|(null)|([{}[\]:,])/g;
    let lastIndex = 0;
    let match: RegExpExecArray | null;

    while ((match = regex.exec(source)) !== null) {
      if (match.index > lastIndex) {
        result.push({ type: "plain", text: source.slice(lastIndex, match.index) });
      }

      if (match[1] !== undefined) {
        result.push({ type: "key", text: match[1] });
      } else if (match[2] !== undefined) {
        result.push({ type: "string", text: match[2] });
      } else if (match[3] !== undefined) {
        result.push({ type: "number", text: match[3] });
      } else if (match[4] !== undefined) {
        result.push({ type: "boolean", text: match[4] });
      } else if (match[5] !== undefined) {
        result.push({ type: "null", text: match[5] });
      } else if (match[6] !== undefined) {
        result.push({ type: "punctuation", text: match[6] });
      }

      lastIndex = match.index + match[0].length;
    }

    if (lastIndex < source.length) {
      result.push({ type: "plain", text: source.slice(lastIndex) });
    }

    return result;
  }

  const TOKEN_CLASSES: Record<TokenType, string> = {
    key: "text-info",
    string: "text-success",
    number: "text-warning",
    boolean: "text-accent-foreground",
    null: "text-destructive/70",
    punctuation: "text-muted-foreground/60",
    plain: "",
  };
</script>

<pre
  class={cn(
    "overflow-x-auto whitespace-pre-wrap rounded glass-border glass-surface p-3 font-mono text-sm",
    className,
  )}
  data-testid="json-viewer"
>{#each tokens as token}<span class={TOKEN_CLASSES[token.type]}>{token.text}</span>{/each}</pre>
