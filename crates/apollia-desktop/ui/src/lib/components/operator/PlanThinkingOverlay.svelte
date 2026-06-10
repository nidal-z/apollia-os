<script lang="ts">
  // Live thinking stream attached to the current plan step.
  //
  // Reuses the quiet collapsible pattern of `components/chat/ReasoningCard.svelte`
  // (the `thinking` kind): no border chrome, a muted toggle header and a muted
  // pre-wrapped text block. The data flows in as props fed by the thinking
  // store on the chat side; no invoke here. While `live` the header reads
  // "Thinking", and on `ThinkingEnded` the parent flips `live` to false so the
  // header settles on the frozen "Thoughts" label. Renders nothing when the
  // content is blank, so a turn without a thinking trace shows no empty box.
  import { t } from "svelte-i18n";
  import { PLAN_SESSION_KEYS } from "$lib/i18n/strings/planSession";

  type Props = {
    /** Live or frozen thinking text for the current turn. */
    content: string;
    /** True while ThinkingStarted has fired and ThinkingEnded has not. */
    live: boolean;
  };
  let { content, live }: Props = $props();

  let expanded = $state(true);
  const hasContent = $derived(content.trim().length > 0);
</script>

{#if hasContent}
  <section class="my-1.5" data-testid="plan-thinking-overlay">
    <button
      type="button"
      class="inline-flex items-center gap-1 text-[11px] text-muted-foreground transition-colors hover:text-foreground"
      aria-expanded={expanded}
      onclick={() => (expanded = !expanded)}
    >
      <span
        class="inline-block leading-none transition-transform duration-150"
        class:rotate-90={expanded}>›</span
      >
      <span class="font-medium">
        {$t(live ? PLAN_SESSION_KEYS.thinkingLive : PLAN_SESSION_KEYS.thinkingDone)}
      </span>
    </button>
    {#if expanded}
      <p
        class="mt-1 whitespace-pre-wrap pl-3 text-[12px] leading-relaxed text-muted-foreground/85"
      >
        {content}
      </p>
    {/if}
  </section>
{/if}
