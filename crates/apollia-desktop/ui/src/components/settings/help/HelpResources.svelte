<script lang="ts">
  /**
   * HelpResources - the "resources & support" card grid.
   *
   * Three resource kinds, each rendered with the correct semantics:
   *   - external (`<a>` via the system opener): the help center, the community,
   *     the issue tracker;
   *   - in-app (`<button>`): the offline keyboard-shortcuts page;
   *   - soon (`<div>`): kept for a surface that is genuinely not published yet,
   *     shown as "available soon" rather than a dead link (repo doctrine).
   *
   * When offline the external cards are disabled (not hidden) and a banner
   * explains why; the in-app card stays usable.
   */
  import { t } from "svelte-i18n";
  import { BookOpen, Keyboard, Globe, Bug, ExternalLink, WifiOff } from "lucide-svelte";
  import { navigateToSettings } from "$lib/router";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";

  interface Props {
    online: boolean;
  }

  let { online }: Props = $props();

  // The operator help center, published from docs/site by CI. Deep-linked to
  // its own section rather than the site root: this card answers "how do I do
  // this in the app", not "what is Apollia".
  const DOCS_URL = "https://docs.apollia.fr/operator-help";
  const COMMUNITY_URL = "https://github.com/Apollia-OS/apollia-os/discussions";
  const REPORT_URL = "https://github.com/Apollia-OS/apollia-os/issues/new";

  type Icon = typeof BookOpen;

  type Resource =
    | { id: string; kind: "external"; icon: Icon; titleKey: string; descKey: string; href: string; warn?: boolean }
    | { id: string; kind: "in-app"; icon: Icon; titleKey: string; descKey: string; run: () => void }
    | { id: string; kind: "soon"; icon: Icon; titleKey: string; descKey: string };

  const resources: Resource[] = [
    {
      id: "docs",
      kind: "external",
      icon: BookOpen,
      titleKey: "settings.help.docs_title",
      descKey: "settings.help.docs_desc",
      href: DOCS_URL,
    },
    {
      id: "shortcuts",
      kind: "in-app",
      icon: Keyboard,
      titleKey: "settings.help.shortcuts_title",
      descKey: "settings.help.shortcuts_desc",
      run: () => navigateToSettings("shortcuts"),
    },
    {
      id: "community",
      kind: "external",
      icon: Globe,
      titleKey: "settings.help.community_title",
      descKey: "settings.help.community_desc",
      href: COMMUNITY_URL,
    },
    {
      id: "report",
      kind: "external",
      icon: Bug,
      titleKey: "settings.help.report_title",
      descKey: "settings.help.report_desc",
      href: REPORT_URL,
      warn: true,
    },
  ];
</script>

{#snippet resourceInner(res: Resource, external: boolean)}
  <span
    class="grid h-8 w-8 shrink-0 place-items-center rounded-md {res.kind === 'external' &&
    res.warn
      ? 'bg-warning/10 text-warning'
      : 'bg-muted text-primary'}"
  >
    <res.icon size={16} strokeWidth={1.75} aria-hidden="true" />
  </span>
  <span class="min-w-0 flex-1">
    <span class="flex items-center gap-1.5 text-body-sm font-semibold text-foreground">
      {$t(res.titleKey)}
      {#if external}
        <ExternalLink
          size={12}
          class="text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
          aria-hidden="true"
        />
      {/if}
    </span>
    <span class="mt-1 block text-body-xs leading-relaxed text-muted-foreground">{$t(res.descKey)}</span>
    <span class="mt-2 block text-caption text-muted-foreground/80">
      {#if res.kind === "external"}
        {#if online}
          {$t("settings.help.opens_external")}
        {:else}
          <span class="inline-flex items-center gap-1 text-warning">
            <WifiOff size={11} strokeWidth={1.75} aria-hidden="true" />
            {$t("settings.help.offline_badge")}
          </span>
        {/if}
      {:else if res.kind === "in-app"}
        {$t("settings.help.opens_in_app")}
      {:else}
        {$t("settings.help.available_soon")}
      {/if}
    </span>
  </span>
{/snippet}

<section class="space-y-2.5" aria-label={$t("settings.help.resources")}>
  <h2 class="px-1 text-overline uppercase text-muted-foreground">
    {$t("settings.help.resources")}
  </h2>

  {#if !online}
    <div
      class="flex items-start gap-3 rounded-xl border border-warning/30 border-l-2 border-l-warning
        bg-warning/5 px-4 py-3"
      data-testid="help-offline-banner"
    >
      <WifiOff size={16} strokeWidth={1.75} class="mt-0.5 shrink-0 text-warning" aria-hidden="true" />
      <div class="min-w-0">
        <p class="text-body-sm font-semibold text-foreground">{$t("settings.help.offline_title")}</p>
        <p class="mt-0.5 text-body-xs leading-relaxed text-muted-foreground">
          {$t("settings.help.offline_body")}
        </p>
      </div>
    </div>
  {/if}

  <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
    {#each resources as res (res.id)}
      {#if res.kind === "external"}
        <a
          href={res.href}
          rel="noreferrer"
          onclick={handleExternalLinkClick}
          class="group flex items-start gap-3 rounded-xl border border-border bg-card p-4
            transition hover:-translate-y-0.5 hover:border-primary/40 hover:bg-muted/30 hover:shadow-md
            focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40
            motion-reduce:transition-none motion-reduce:hover:translate-y-0
            aria-disabled:pointer-events-none aria-disabled:opacity-55"
          aria-disabled={!online}
          tabindex={online ? 0 : -1}
          data-testid="help-link-{res.id}"
        >
          {@render resourceInner(res, true)}
        </a>
      {:else if res.kind === "in-app"}
        <button
          type="button"
          onclick={res.run}
          class="group flex items-start gap-3 rounded-xl border border-border bg-card p-4 text-left
            transition hover:-translate-y-0.5 hover:border-primary/40 hover:bg-muted/30 hover:shadow-md
            focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40
            motion-reduce:transition-none motion-reduce:hover:translate-y-0"
          data-testid="help-link-{res.id}"
        >
          {@render resourceInner(res, false)}
        </button>
      {:else}
        <div
          class="flex items-start gap-3 rounded-xl border border-border bg-card p-4 opacity-90"
          data-testid="help-link-{res.id}"
        >
          {@render resourceInner(res, false)}
        </div>
      {/if}
    {/each}
  </div>
</section>
