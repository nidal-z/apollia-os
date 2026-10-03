<script lang="ts">
  /**
   * AboutBlock - the "About" block at the bottom of Settings > General: what
   * runs on this machine, the commitment, the licence, the credits, and the
   * links to the documentation, the source and the contact address.
   */
  import { t, locale } from "svelte-i18n";
  import {
    Globe,
    Scale,
    Package,
    BookOpen,
    Github,
    Mail,
    ShieldCheck,
    ExternalLink,
    Copy,
    Check,
  } from "lucide-svelte";
  import { docsUrlFor } from "$lib/utils/docsUrl";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import AboutCredits from "./AboutCredits.svelte";
  import AboutEthos from "./AboutEthos.svelte";

  const GITHUB_URL = "https://github.com/Apollia-OS/apollia-os";
  // The documentation site, built and published from docs/site by CI. The
  // previous target was the repository wiki, which the corpus consolidation
  // retired: the link shipped dead in the binary. The home page exists in both
  // locales, so the card follows the interface language.
  const DOCS_URL = $derived(docsUrlFor($locale, "/"));
  const CONTACT_EMAIL = "contact@apollia.fr";
  const LICENSE_SPDX = "MIT OR Apache-2.0";

  async function copyContact(): Promise<void> {
    try {
      await navigator.clipboard.writeText(CONTACT_EMAIL);
      addToast($t("settings.about.copy_success"), "success");
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }
</script>

{#snippet linkCard(
  href: string,
  glyph: typeof BookOpen,
  title: string,
  desc: string,
  testid: string,
)}
  {@const Glyph = glyph}
  <a
    {href}
    rel="noreferrer"
    onclick={handleExternalLinkClick}
    class="group flex items-center gap-3 rounded-lg border border-border bg-card p-3 transition-colors hover:border-primary/40 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
    data-testid={testid}
  >
    <span class="grid h-9 w-9 shrink-0 place-items-center rounded-md bg-muted text-primary">
      <Glyph size={17} strokeWidth={1.75} aria-hidden="true" />
    </span>
    <span class="min-w-0 flex-1">
      <span class="flex items-center gap-1.5 text-body-sm font-medium text-foreground">
        {title}
        <ExternalLink
          size={12}
          class="text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
          aria-hidden="true"
        />
      </span>
      <span class="block truncate text-caption text-muted-foreground">{desc}</span>
    </span>
  </a>
{/snippet}

<div class="flex flex-col gap-4" data-testid="about-section">
  <h2 class="px-1 pt-2 text-overline uppercase text-muted-foreground">
    {$t("settings.general.about_title")}
  </h2>

  <!-- What runs on this machine -->
  <SettingsSection
    title={$t("settings.about.local_title")}
    description={$t("settings.about.local_desc")}
    data-testid="about-local"
  >
    {#snippet icon()}<ShieldCheck size={15} strokeWidth={1.75} />{/snippet}
    <ul class="m-0 list-none space-y-2 p-0">
      {#each ["inference", "transcription", "memory", "audit"] as item (item)}
        <li class="flex items-start gap-2.5">
          <Check size={14} strokeWidth={2.2} class="mt-0.5 shrink-0 text-success" aria-hidden="true" />
          <span class="text-body-sm leading-relaxed text-muted-foreground">
            {$t(`settings.about.local_${item}`)}
          </span>
        </li>
      {/each}
    </ul>
  </SettingsSection>

  <AboutEthos />

  <!-- License -->
  <SettingsSection
    title={$t("settings.about.license_title")}
    description={$t("settings.about.license_body")}
    bodyLayout="flush"
    data-testid="about-license"
  >
    {#snippet icon()}<Scale size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow label={$t("settings.about.kv_license")} border={false}>
      {#snippet control()}
        <span class="inline-flex items-center rounded-full leading-none border border-border bg-muted/50 px-2.5 py-0.5 font-mono text-caption text-foreground">
          {LICENSE_SPDX}
        </span>
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>

  <!-- Built on -->
  <SettingsSection
    title={$t("settings.about.credits_title")}
    description={$t("settings.about.credits_desc")}
  >
    {#snippet icon()}<Package size={15} strokeWidth={1.75} />{/snippet}
    <AboutCredits />
  </SettingsSection>

  <!-- Resources -->
  <SettingsSection title={$t("settings.about.links_title")} data-testid="about-links">
    {#snippet icon()}<Globe size={15} strokeWidth={1.75} />{/snippet}
    <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
      {@render linkCard(
        DOCS_URL,
        BookOpen,
        $t("settings.about.docs_title"),
        $t("settings.about.docs_desc"),
        "about-link-docs",
      )}
      {@render linkCard(
        GITHUB_URL,
        Github,
        $t("settings.about.github_title"),
        $t("settings.about.github_desc"),
        "about-link-github",
      )}
      <button
        type="button"
        onclick={copyContact}
        class="group flex items-center gap-3 rounded-lg border border-border bg-card p-3 text-left transition-colors hover:border-primary/40 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        data-testid="about-copy-contact"
      >
        <span class="grid h-9 w-9 shrink-0 place-items-center rounded-md bg-muted text-primary">
          <Mail size={17} strokeWidth={1.75} aria-hidden="true" />
        </span>
        <span class="min-w-0 flex-1">
          <span class="flex items-center gap-1.5 text-body-sm font-medium text-foreground">
            {$t("settings.about.contact_title")}
            <Copy
              size={12}
              class="text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
              aria-hidden="true"
            />
          </span>
          <span class="block truncate font-mono text-caption text-muted-foreground">{CONTACT_EMAIL}</span>
        </span>
      </button>
    </div>
  </SettingsSection>
</div>
