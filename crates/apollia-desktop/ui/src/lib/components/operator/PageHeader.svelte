<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    /** Uppercase mono kicker text. */
    kicker?: string;
    title: string;
    subtitle?: string;
    /** Right-aligned action area (buttons). */
    actions?: Snippet;
  }

  let { kicker, title, subtitle, actions }: Props = $props();
</script>

<div
  class="flex items-start gap-4 px-8 pt-10 pb-5 border-b border-border/60"
>
  <div class="flex-1 min-w-0">
    {#if kicker}
      <div
        class="font-mono text-[10.5px] tracking-[1.5px] font-semibold text-muted-foreground/70 uppercase mb-1.5"
      >
        {kicker}
      </div>
    {/if}
    <h1
      class="m-0 text-foreground"
      style="font-size: 28px; font-weight: 600; letter-spacing: -0.6px; line-height: 1.15;"
    >
      {title}
    </h1>
    {#if subtitle}
      <p
        class="mt-2 text-[13.5px] leading-[1.5] text-muted-foreground max-w-[620px]"
      >
        {subtitle}
      </p>
    {/if}
  </div>
  {#if actions}
    <div class="flex items-center gap-2">
      {@render actions()}
    </div>
  {/if}
</div>
