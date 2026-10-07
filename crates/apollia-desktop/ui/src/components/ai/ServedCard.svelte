<script lang="ts">
  /**
   * ServedCard - the card of something a client can call: a model in memory,
   * an agent, a workflow. One layout for all of them: the badges, the name
   * with what follows it, the facts and the actions on the right, and an
   * optional second line under them.
   */
  import type { Snippet } from "svelte";

  interface Props {
    testid: string;
    /** The badges above the name; each explains itself on hover. */
    badges: Snippet;
    /** The name, and the icons that follow it. */
    identity: Snippet;
    /** What the card states on the right, before its actions. */
    facts?: Snippet;
    /** The buttons on the right. */
    actions?: Snippet;
    /** A second line: what is done with the identifier, where the detail is. */
    footer?: Snippet;
  }

  let { testid, badges, identity, facts, actions, footer }: Props = $props();
</script>

<div class="space-y-2 rounded-lg border border-border bg-card p-3" data-testid={testid}>
  <div class="flex flex-wrap items-center justify-between gap-3">
    <div class="min-w-0 space-y-1.5">
      <div class="flex flex-wrap items-center gap-1.5">
        {@render badges()}
      </div>
      <div class="flex items-center gap-2">
        {@render identity()}
      </div>
    </div>
    {#if facts || actions}
      <div class="flex shrink-0 items-center gap-4">
        {@render facts?.()}
        {@render actions?.()}
      </div>
    {/if}
  </div>
  {#if footer}
    <div class="flex flex-wrap items-center justify-between gap-3">
      {@render footer()}
    </div>
  {/if}
</div>
