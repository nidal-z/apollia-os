<script lang="ts">
  /**
   * Source cards - zone 3 of the assistant turn.
   *
   * A responsive 2-column grid of compact cards for the web pages the agent
   * consulted (recovered and deduplicated by `extractSources`). Each card is a
   * favicon letter tile + "N · domain" + a 2-line title clamp, and opens the URL
   * through the app's external opener. This replaces the heavy inline web_read
   * dump that used to live inside the reasoning trace.
   */

  import { t } from "svelte-i18n";
  import type { SourceCard } from "$lib/chat/reasoning";
  import { avatarHue } from "$lib/components/ui/avatar/Avatar.svelte";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { fly } from "svelte/transition";
  import { quintOut } from "svelte/easing";

  interface Props {
    sources: SourceCard[];
  }

  let { sources }: Props = $props();
</script>

{#if sources.length > 0}
  <section class="mt-4 w-full" data-testid="source-cards">
    <h4
      class="tb-header-rule mb-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground/60"
    >
      {$t("chat.activity.sources")}
    </h4>
    <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
      {#each sources as source, i (source.url)}
        <a
          href={source.url}
          target="_blank"
          rel="noopener noreferrer"
          onclick={handleExternalLinkClick}
          in:fly={{ y: 8, duration: 220, delay: i * 30, easing: quintOut }}
          class="group flex items-start gap-2.5 rounded-lg border border-border bg-surface-1 px-3 py-2.5
            no-underline transition-all duration-200 ease-out hover:-translate-y-0.5 hover:border-primary/50
            hover:shadow-[0_8px_22px_-14px_hsl(var(--primary)/0.5)]
            focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
          data-testid="source-card-{source.index}"
        >
          <span
            class="mt-0.5 flex h-5 w-5 flex-none items-center justify-center rounded-md
              text-[10px] font-bold text-white"
            style="background: hsl({avatarHue(source.domain)} 55% 48%);"
            aria-hidden="true"
          >
            {source.domain.charAt(0).toUpperCase()}
          </span>
          <span class="min-w-0">
            <span
              class="block text-[10.5px] font-semibold tabular-nums text-muted-foreground/60"
            >
              {source.index} · {source.domain}
            </span>
            <span
              class="mt-0.5 block text-[12.5px] font-medium leading-snug text-foreground line-clamp-2"
            >
              {source.title}
            </span>
          </span>
        </a>
      {/each}
    </div>
  </section>
{/if}
