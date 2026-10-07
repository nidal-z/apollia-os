<script lang="ts" module>
  /**
   * The title of a reasoning block: still thinking, thought for a measured
   * time, or thought with no time measured (a conversation reopened later).
   */
  export function reasoningTitleKey(input: {
    live: boolean;
    durationMs: number | null | undefined;
    kind: "thinking" | "rationale";
  }): { key: string; withDuration: boolean } {
    if (input.live) return { key: "chat.reasoning.thinking_live", withDuration: false };
    if (input.kind === "rationale") return { key: "chat.reasoning.rationale_label", withDuration: false };
    if (input.durationMs != null && input.durationMs >= 0) {
      return { key: "chat.reasoning.thought_for", withDuration: true };
    }
    return { key: "chat.reasoning.thought", withDuration: false };
  }

  /**
   * The vignette of a reasoning block: the fade over the clipped edge of a
   * thought still arriving, once its text runs past the block. A finished
   * thought is shown whole and has no edge to fade; the setting of Appearance
   * turns the fade off.
   */
  export function showsVignette(input: {
    enabled: boolean;
    live: boolean;
    clipped: boolean;
  }): boolean {
    return input.enabled && input.live && input.clipped;
  }
</script>

<script lang="ts">
  /**
   * One reasoning fragment of a turn, as a foldable block titled by how long
   * it took ("Thought for 4 s"). Folded, it is a quiet line among the steps;
   * unfolded, the same title heads a filled block holding the thought.
   */

  import { t } from "svelte-i18n";
  import { ChevronRight } from "lucide-svelte";
  import MarkdownContent from "$lib/components/ui/markdown/MarkdownContent.svelte";
  import { formatThoughtDuration } from "$lib/chat/toolStep";
  import { reasoningVignette } from "$lib/stores/chatAppearance";

  interface Props {
    content: string;
    kind?: "thinking" | "rationale";
    durationMs?: number | null;
    expanded: boolean;
    ontoggle: () => void;
    /** The fragment is still streaming: no duration, plain text. */
    live?: boolean;
  }

  let {
    content,
    kind = "thinking",
    durationMs = null,
    expanded,
    ontoggle,
    live = false,
  }: Props = $props();

  let thought = $state<HTMLParagraphElement | undefined>(undefined);
  let clipped = $state(false);
  $effect(() => {
    // Re-measured on every chunk: the thought grows until it passes the block.
    void content;
    clipped = thought !== undefined && thought.scrollHeight > thought.clientHeight;
  });
  const vignette = $derived(showsVignette({ enabled: $reasoningVignette, live, clipped }));

  const title = $derived.by<string>(() => {
    const { key, withDuration } = reasoningTitleKey({ live, durationMs, kind });
    return withDuration && durationMs != null
      ? $t(key, { values: { duration: formatThoughtDuration(durationMs) } })
      : $t(key);
  });
</script>

<div
  class="flex flex-col rounded-lg transition-colors {expanded ? 'bg-surface-2 px-3 py-2' : ''}"
  data-testid="reasoning-card-{kind}"
>
  <button
    type="button"
    class="flex w-full items-center gap-1.5 rounded-md py-1 text-left text-body-sm text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    aria-expanded={expanded}
    aria-label={$t("chat.reasoning.toggle_thought")}
    data-testid="reasoning-block-toggle"
    onclick={ontoggle}
  >
    <ChevronRight
      class="h-3.5 w-3.5 flex-none transition-transform duration-fast {expanded ? 'rotate-90' : ''}"
      aria-hidden="true"
    />
    <span data-testid="reasoning-block-title">{title}</span>
  </button>
  {#if expanded}
    <div
      class="reasoning-caption min-w-0 pb-1 pl-5 text-body-xs leading-relaxed text-muted-foreground"
      data-testid="reasoning-thought"
    >
      {#if live}
        <div class="relative" data-vignette={vignette}>
          <p bind:this={thought} class="max-h-40 overflow-hidden whitespace-pre-wrap">{content}</p>
          {#if vignette}
            <span
              class="pointer-events-none absolute inset-x-0 bottom-0 h-8 bg-gradient-to-t from-surface-2 to-transparent"
              aria-hidden="true"
              data-testid="reasoning-vignette"
            ></span>
          {/if}
        </div>
      {:else}
        <MarkdownContent {content} />
      {/if}
    </div>
  {/if}
</div>
