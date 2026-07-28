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
  import {
    Info,
    RefreshCw,
    Globe,
    Scale,
    Package,
    BookOpen,
    Github,
    Mail,
    ExternalLink,
    Copy,
    Check,
  } from "lucide-svelte";
  import { systemInfoStore, settingsLoaders } from "$lib/stores/settings";
  import { handleExternalLinkClick } from "$lib/utils/externalLink";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";
  import AboutHero from "../../components/settings/about/AboutHero.svelte";
  import UpdateCheckPanel from "../../components/settings/UpdateCheckPanel.svelte";
  import AboutCredits from "../../components/settings/about/AboutCredits.svelte";
  import AboutEthos from "../../components/settings/about/AboutEthos.svelte";
  import CopyValue from "../../components/settings/about/CopyValue.svelte";

  const GITHUB_URL = "https://github.com/Apollia-OS/apollia-os";
  const DOCS_URL = "https://github.com/Apollia-OS/apollia-os/wiki";
  const CONTACT_EMAIL = "contact@apollia.fr";
  const LICENSE_SPDX = "MIT OR Apache-2.0";
  const ENGINE = "llama-server (llama.cpp)";
  const STT_ENGINE = "whisper-rs (GGML)";

  const version = $derived($systemInfoStore.data?.version ?? null);
  const os = $derived($systemInfoStore.data?.os ?? null);
  const pythonPath = $derived($systemInfoStore.data?.python_path ?? null);
  const unavailable = $derived(
    $systemInfoStore.data === null && $systemInfoStore.error !== null,
  );

  // The channel is inferred from the version string's prerelease tag (the part
  // after the first "-"), never fabricated: no tag means a stable build.
  const channelLabel = $derived.by(() => {
    if (!version) return null;
    return version.includes("-")
      ? $t("settings.about.channel_prerelease")
      : $t("settings.about.channel_stable");
  });

  // A copy-ready diagnostic block composed only from real, present fields.
  const diagnostic = $derived.by(() => {
    const lines = [`Apollia OS v${version ?? "unknown"}`, `Platform: ${os ?? "unknown"}`];
    if (pythonPath) lines.push(`Python: ${pythonPath}`);
    lines.push(`Engine: ${ENGINE}`, `Transcription: ${STT_ENGINE}`, `License: ${LICENSE_SPDX}`);
    return lines.join("\n");
  });

  let diagCopied = $state(false);
  let diagTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(() => {
    void settingsLoaders.systemInfo();
  });

  async function copyDiagnostic(): Promise<void> {
    try {
      await navigator.clipboard.writeText(diagnostic);
      diagCopied = true;
      addToast($t("settings.about.copy_success"), "success");
      clearTimeout(diagTimer);
      diagTimer = setTimeout(() => (diagCopied = false), 1500);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

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

<SettingsSubPage route="about" data-testid="about-section">
  <AboutHero {version} {os} {channelLabel} />

  <!-- Version and build -->
  <SettingsSection
    title={$t("settings.about.build_title")}
    description={$t("settings.about.build_desc")}
    bodyLayout="flush"
    data-testid="about-build"
  >
    {#snippet icon()}<Info size={15} strokeWidth={1.75} />{/snippet}

    {#if unavailable}
      <p class="py-3 text-body-sm text-muted-foreground" data-testid="about-info-unavailable">
        {$t("settings.about.info_unavailable")}
      </p>
    {:else}
      <SettingsFieldRow label={$t("settings.about.kv_version")}>
        {#snippet control()}
          {#if version}
            <CopyValue value={version} label={$t("settings.about.kv_version")} data-testid="about-copy-version" />
          {:else}
            <span class="text-body-sm text-muted-foreground">{$t("settings.about.version_unknown")}</span>
          {/if}
        {/snippet}
      </SettingsFieldRow>

      <SettingsFieldRow label={$t("settings.about.kv_platform")}>
        {#snippet control()}
          {#if os}
            <CopyValue value={os} label={$t("settings.about.kv_platform")} data-testid="about-copy-platform" />
          {:else}
            <span class="text-body-sm text-muted-foreground">-</span>
          {/if}
        {/snippet}
      </SettingsFieldRow>

      {#if pythonPath}
        <SettingsFieldRow label={$t("settings.about.kv_python")}>
          {#snippet control()}
            <CopyValue value={pythonPath} label={$t("settings.about.kv_python")} data-testid="about-copy-python" />
          {/snippet}
        </SettingsFieldRow>
      {/if}

      <SettingsFieldRow label={$t("settings.about.kv_engine")}>
        {#snippet control()}
          <span class="font-mono text-body-sm text-foreground">{ENGINE}</span>
        {/snippet}
      </SettingsFieldRow>

      <SettingsFieldRow label={$t("settings.about.kv_stt")} border={false}>
        {#snippet control()}
          <span class="font-mono text-body-sm text-foreground">{STT_ENGINE}</span>
        {/snippet}
      </SettingsFieldRow>
    {/if}

    {#snippet footer()}
      <Button
        variant="ghost"
        size="sm"
        onclick={copyDiagnostic}
        data-testid="about-copy-diagnostics"
      >
        {#snippet icon()}
          {#if diagCopied}
            <Check size={14} strokeWidth={2.2} class="text-success" />
          {:else}
            <Copy size={14} strokeWidth={1.9} />
          {/if}
        {/snippet}
        {$t("settings.about.copy_diagnostics")}
      </Button>
      <span class="ml-auto text-caption text-muted-foreground">
        {$t("settings.about.copy_hint")}
      </span>
    {/snippet}
  </SettingsSection>

  <!-- Updates -->
  <SettingsSection
    title={$t("settings.about.updates_title")}
    description={$t("settings.about.updates_desc")}
    data-testid="about-updates"
  >
    {#snippet icon()}<RefreshCw size={15} strokeWidth={1.75} />{/snippet}
    <UpdateCheckPanel variant="bare" />
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
        <span class="inline-flex items-center rounded-full border border-border bg-muted/50 px-2.5 py-0.5 font-mono text-caption text-foreground">
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

  <AboutEthos />
</SettingsSubPage>
