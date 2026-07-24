<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.about",
    icon: "badge-info",
    group: "settings.nav.cluster_help",
    cluster: "about",
  } as const;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { BookOpen, Github, Scale, Mail, Copy, ExternalLink } from "lucide-svelte";
  import { Card } from "$lib/components/ui/card";
  import { systemInfoStore, settingsLoaders } from "$lib/stores/settings";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { addToast } from "$lib/components/ui/toast";

  const GITHUB_URL = "https://github.com/Apollia-OS/apollia-os";
  const DOCS_URL = "https://github.com/Apollia-OS/apollia-os/wiki";
  const CONTACT_PRIMARY = "contact@apollia.fr";
  const CONTACT_ADMIN = "admin@apollia.fr";

  const version = $derived($systemInfoStore.data?.version ?? null);
  const os = $derived($systemInfoStore.data?.os ?? null);

  onMount(() => {
    void settingsLoaders.systemInfo();
  });

  async function copyEmail(address: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(address);
      addToast($t("settings.about.email_copied"), "success");
    } catch {
      addToast($t("settings.about.email_copy_failed"), "error");
    }
  }
</script>

<section class="mx-auto max-w-[720px] space-y-5" data-testid="about-section">
  <!-- Identity hero -->
  <Card class="rounded-xl p-6" data-testid="about-hero">
    <div class="flex flex-col items-center gap-3 text-center sm:flex-row sm:items-center sm:gap-5 sm:text-left">
      <img
        src="/logo.svg"
        alt={$t("settings.about.logo_alt")}
        width="64"
        height="64"
        class="h-16 w-16 shrink-0 rounded-2xl"
      />
      <div class="min-w-0">
        <h2
          class="m-0 text-foreground"
          style="font-size: 22px; font-weight: 600; letter-spacing: -0.4px;"
        >
          Apollia OS
        </h2>
        <p class="mt-1 text-[13px] leading-relaxed text-muted-foreground">
          {$t("settings.about.tagline")}
        </p>
        <div class="mt-3 flex flex-wrap items-center justify-center gap-2 sm:justify-start">
          {#if version}
            <span
              class="inline-flex items-center rounded-full border border-border bg-muted/50 px-2.5 py-0.5 font-mono text-[11px] text-foreground"
              data-testid="about-version"
            >
              v{version}
            </span>
          {/if}
          {#if os}
            <span
              class="inline-flex items-center rounded-full border border-border bg-muted/30 px-2.5 py-0.5 font-mono text-[11px] text-muted-foreground"
              data-testid="about-os"
            >
              {os}
            </span>
          {/if}
        </div>
      </div>
    </div>
  </Card>

  <!-- Description -->
  <Card class="rounded-xl p-5" data-testid="about-description">
    <p class="text-[13px] leading-[1.65] text-muted-foreground">
      {$t("settings.about.description")}
    </p>
  </Card>

  <!-- Links -->
  <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
    <a
      href={DOCS_URL}
      rel="noreferrer"
      onclick={handleExternalLinkClick}
      class="group flex items-center gap-3 rounded-xl border border-border bg-card p-4 transition-colors hover:border-primary/40 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
      data-testid="about-link-docs"
    >
      <span class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-muted text-primary">
        <BookOpen size={18} strokeWidth={1.75} aria-hidden="true" />
      </span>
      <span class="min-w-0 flex-1">
        <span class="flex items-center gap-1.5 text-[13px] font-medium text-foreground">
          {$t("settings.about.docs_title")}
          <ExternalLink size={12} class="text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" aria-hidden="true" />
        </span>
        <span class="block text-[11.5px] text-muted-foreground">{$t("settings.about.docs_desc")}</span>
      </span>
    </a>

    <a
      href={GITHUB_URL}
      rel="noreferrer"
      onclick={handleExternalLinkClick}
      class="group flex items-center gap-3 rounded-xl border border-border bg-card p-4 transition-colors hover:border-primary/40 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
      data-testid="about-link-github"
    >
      <span class="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-muted text-foreground">
        <Github size={18} strokeWidth={1.75} aria-hidden="true" />
      </span>
      <span class="min-w-0 flex-1">
        <span class="flex items-center gap-1.5 text-[13px] font-medium text-foreground">
          {$t("settings.about.github_title")}
          <ExternalLink size={12} class="text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" aria-hidden="true" />
        </span>
        <span class="block text-[11.5px] text-muted-foreground">{$t("settings.about.github_desc")}</span>
      </span>
    </a>
  </div>

  <!-- License -->
  <Card class="rounded-xl p-5" data-testid="about-license">
    <div class="flex items-start gap-3">
      <Scale size={18} strokeWidth={1.75} class="mt-0.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      <div class="min-w-0">
        <h3 class="text-[13px] font-medium text-foreground">{$t("settings.about.license_title")}</h3>
        <p class="mt-1 text-[12px] leading-relaxed text-muted-foreground">
          {$t("settings.about.license_body")}
        </p>
      </div>
    </div>
  </Card>

  <!-- Contact -->
  <Card class="rounded-xl p-5" data-testid="about-contact">
    <div class="flex items-start gap-3">
      <Mail size={18} strokeWidth={1.75} class="mt-0.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      <div class="min-w-0 flex-1">
        <h3 class="text-[13px] font-medium text-foreground">{$t("settings.about.contact_title")}</h3>
        <p class="mt-1 text-[12px] leading-relaxed text-muted-foreground">
          {$t("settings.about.contact_body")}
        </p>
        <div class="mt-3 space-y-1.5">
          {#each [CONTACT_PRIMARY, CONTACT_ADMIN] as address (address)}
            <div class="flex items-center gap-2">
              <button
                type="button"
                onclick={() => copyEmail(address)}
                class="group inline-flex items-center gap-1.5 rounded-md text-[12px] font-mono text-foreground transition-colors hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
                title={$t("settings.about.copy_email")}
                data-testid="about-copy-{address}"
              >
                <span>{address}</span>
                <Copy size={12} class="opacity-0 transition-opacity group-hover:opacity-100" aria-hidden="true" />
              </button>
            </div>
          {/each}
        </div>
      </div>
    </div>
  </Card>
</section>
