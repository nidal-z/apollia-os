<script lang="ts">
  import type { Snippet } from "svelte";

  type Tone = "primary" | "secondary" | "success" | "info" | "neutral";

  interface Props {
    icon?: Snippet;
    title: string;
    desc?: string;
    action?: Snippet;
    tone?: Tone;
  }

  let { icon, title, desc, action, tone = "primary" }: Props = $props();

  const TONE_CLASS: Record<Tone, string> = {
    primary: "bg-primary/5 text-primary",
    secondary: "bg-secondary/10 text-secondary-foreground",
    success: "bg-success/10 text-success-a11y",
    info: "bg-info/10 text-info",
    neutral: "bg-muted text-muted-foreground",
  };
</script>

<div
  class="flex flex-col items-center justify-center text-center gap-2.5 px-10 py-14"
>
  {#if icon}
    <div
      class="inline-flex items-center justify-center w-[52px] h-[52px] rounded-2xl {TONE_CLASS[
        tone
      ]}"
    >
      {@render icon()}
    </div>
  {/if}
  <h4 class="m-0 text-[15px] font-semibold text-foreground">{title}</h4>
  {#if desc}
    <p
      class="m-0 text-[12.5px] text-muted-foreground max-w-[380px] leading-[1.5]"
    >
      {desc}
    </p>
  {/if}
  {#if action}
    <div class="mt-1.5">{@render action()}</div>
  {/if}
</div>
