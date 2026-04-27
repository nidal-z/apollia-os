<!--
  RichLinkPreview — OG card for a standalone link.

  Loads metadata via `fetchLinkPreview` (Tauri backend).  While loading or on
  error, falls back to rendering a plain link so the content stays readable.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { fetchLinkPreview, type LinkPreview } from "$lib/chat/linkPreview";
  import { ExternalLink } from "lucide-svelte";

  interface Props {
    url: string;
  }

  let { url }: Props = $props();

  let preview = $state<LinkPreview | null>(null);
  let loading = $state(true);

  onMount(async () => {
    try {
      preview = await fetchLinkPreview(url);
    } finally {
      loading = false;
    }
  });

  function displayHost(u: string): string {
    try {
      return new URL(u).hostname.replace(/^www\./, "");
    } catch {
      return u;
    }
  }

  function openInBrowser(): void {
    try {
      window.open(url, "_blank", "noopener,noreferrer");
    } catch {
      // Non-fatal.
    }
  }
</script>

{#if loading || preview === null || (preview.title === null && preview.description === null && preview.image === null)}
  <a
    href={url}
    target="_blank"
    rel="noopener noreferrer"
    class="inline-flex items-center gap-1 text-primary hover:underline break-all"
    data-testid="link-preview-fallback"
  >
    <ExternalLink size={11} />
    {url}
  </a>
{:else}
  <button
    type="button"
    class="block w-full text-left rounded-xl border border-border/40 bg-card/80 px-3 py-2.5
      hover:border-primary/40 hover:bg-card/95 transition-all
      shadow-[0_2px_8px_-4px_rgba(0,0,0,0.08)]"
    onclick={openInBrowser}
    data-testid="link-preview-card"
  >
    <div class="flex gap-3">
      {#if preview.image}
        <img
          src={preview.image}
          alt=""
          class="h-16 w-16 flex-shrink-0 rounded-lg object-cover bg-muted"
          loading="lazy"
          onerror={(e) => ((e.currentTarget as HTMLImageElement).style.display = "none")}
        />
      {/if}
      <div class="min-w-0 flex-1">
        <div class="mb-1 flex items-center gap-1.5 text-[10px] text-muted-foreground/60">
          {#if preview.favicon}
            <img
              src={preview.favicon}
              alt=""
              class="h-3 w-3 flex-shrink-0"
              onerror={(e) => ((e.currentTarget as HTMLImageElement).style.display = "none")}
            />
          {/if}
          <span class="truncate">{preview.site_name ?? displayHost(preview.url)}</span>
        </div>
        {#if preview.title}
          <p class="text-xs font-medium text-foreground line-clamp-2">
            {preview.title}
          </p>
        {/if}
        {#if preview.description}
          <p class="mt-0.5 text-[11px] text-muted-foreground line-clamp-2">
            {preview.description}
          </p>
        {/if}
      </div>
    </div>
  </button>
{/if}

<style>
  .line-clamp-2 {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
