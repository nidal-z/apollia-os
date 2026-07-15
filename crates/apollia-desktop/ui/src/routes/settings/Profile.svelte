<script lang="ts" context="module">
  import { Card } from "$lib/components/ui/card";
  export const meta = {
    title: "settings.nav.profile",
    icon: "user",
    group: "settings.nav.cluster_personalization",
    cluster: "personalization",
  } as const;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { User, ShieldAlert, Cpu, Briefcase, Settings2, AlertTriangle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Select } from "$lib/components/ui/select";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { addToast } from "$lib/components/ui/toast/store";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import { onboardingModalOpen, onboardingResumeMode } from "$lib/stores/onboarding";

  // ---------------------------------------------------------------------------
  // Profile keys are stored flat in `__user__`.  The schema is
  // declared in Rust (PROFILE_SCHEMA); the form below mirrors it manually.
  // ---------------------------------------------------------------------------

  // Sensitive keys: their value impacts agent behavior and tool permissions.
  // Editing one does NOT auto-rederive rules - the user must rerun the
  // onboarding agent to have updated proposals applied.
  const SENSITIVE_KEYS = new Set<string>([
    "constraints.sovereignty",
    "constraints.compliance",
    "agents.hitl",
    "tech.integrations",
  ]);

  interface ProfileEntryView {
    key: string;
    value: string;
    written_by: string;
    created_at: string;
    updated_at: string;
    in_schema: boolean;
  }

  interface UserProfileView {
    schema_entries: ProfileEntryView[];
    extras: ProfileEntryView[];
    entries: ProfileEntryView[];
    last_updated_at: string | null;
  }

  let loading = $state(true);
  let saving = $state<Record<string, boolean>>({});
  let values = $state<Record<string, string>>({});
  let sources = $state<Record<string, string>>({});
  let resetOpen = $state(false);
  let resetting = $state(false);
  let enrichmentDismissed = $state(false);

  // "Complete your profile" nudge: once calibration (name + role) is done but
  // optional Tier 2 fields are still empty, offer to resume the onboarding
  // agent (which preserves collected data) instead of filling the form by hand.
  const TIER2_KEYS = [
    "goals",
    "domain.sector",
    "domain.team_size",
    "tech.stack",
    "tech.proficiency",
    "tools.daily",
    "tech.integrations",
    "preferences.language",
    "preferences.llm",
    "agents.domains",
    "agents.trigger",
    "constraints.compliance",
  ];
  const calibrationDone = $derived(
    !!values["name"]?.trim() && !!values["role"]?.trim(),
  );
  const tier2MissingCount = $derived(
    TIER2_KEYS.filter((k) => !values[k]?.trim()).length,
  );
  const showEnrichmentBanner = $derived(
    !enrichmentDismissed && calibrationDone && tier2MissingCount > 0,
  );

  function resumeEnrichment(): void {
    onboardingResumeMode.set(true);
    onboardingModalOpen.set(true);
  }

  // ── Loading ───────────────────────────────────────────────────────────────
  onMount(async () => {
    try {
      const profile = await invoke<UserProfileView>("get_profile");
      const valMap: Record<string, string> = {};
      const srcMap: Record<string, string> = {};
      for (const e of profile.entries ?? []) {
        valMap[e.key] = e.value;
        srcMap[e.key] = e.written_by;
      }
      values = valMap;
      sources = srcMap;
    } catch {
      // Profile not initialized → empty form, not an error.
      values = {};
      sources = {};
    } finally {
      loading = false;
    }
  });

  // ── Persistence ───────────────────────────────────────────────────────────
  async function saveKey(key: string, value: string) {
    // Skip no-op writes so a blur without modification preserves the original
    // `written_by` provenance (onboarding / agent badges would otherwise be
    // overwritten with "user" on every focus traversal).
    const current = values[key] ?? "";
    const next = value.trim();
    if (next === current) return;

    saving = { ...saving, [key]: true };
    try {
      if (value.trim() === "") {
        try {
          await invoke("delete_profile_entry", { key });
        } catch {
          // If the entry did not exist, ignore NOT_FOUND.
        }
        delete values[key];
        delete sources[key];
        values = { ...values };
        sources = { ...sources };
      } else {
        await invoke<ProfileEntryView>("set_profile_entry", {
          request: { key, value: value.trim() },
        });
        values = { ...values, [key]: value.trim() };
        // Writes from this form are always tagged `user` server-side.
        sources = { ...sources, [key]: "user" };
      }
      if (SENSITIVE_KEYS.has(key)) {
        addToast($t("settings.profile.toast.updated_sensitive"), "info");
      } else {
        addToast($t("settings.profile.toast.updated"), "success");
      }
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      saving = { ...saving, [key]: false };
    }
  }

  // Helpers -------------------------------------------------------------------

  function val(key: string): string {
    return values[key] ?? "";
  }

  function isSet(key: string, value: string): boolean {
    return val(key) === value;
  }

  /** Toggle a comma-separated list value (used for compliance/integrations checkboxes). */
  async function toggleListEntry(key: string, entry: string, checked: boolean) {
    const current = val(key);
    const items = current
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    const set = new Set(items);
    if (checked) set.add(entry);
    else set.delete(entry);
    await saveKey(key, Array.from(set).join(", "));
  }

  function listIncludes(key: string, entry: string): boolean {
    return val(key)
      .split(",")
      .map((s) => s.trim())
      .includes(entry);
  }

  // ── Source badges ─────────────────────────────────────────────────────────
  // The `written_by` provenance is one of `onboarding`, `user`, or
  // `agent:<name>` (e.g. `agent:chat-extractor`).
  function sourceLabel(src: string | undefined): string {
    if (!src) return "";
    if (src === "onboarding") return $t("settings.profile.source.onboarding");
    if (src === "user") return $t("settings.profile.source.user");
    if (src.startsWith("agent:")) return $t("settings.profile.source.agent");
    return "";
  }

  function sourceBadgeClasses(src: string | undefined): string {
    const base =
      "inline-flex items-center rounded-full px-1.5 py-px text-[9px] font-medium uppercase tracking-wide";
    if (src === "onboarding") return `${base} bg-primary/10 text-primary`;
    if (src === "user")
      return `${base} bg-emerald-500/10 text-emerald-600 dark:text-emerald-400`;
    if (src && src.startsWith("agent:"))
      return `${base} bg-amber-500/10 text-amber-600 dark:text-amber-400`;
    return `${base} bg-muted text-muted-foreground`;
  }

  // ── Reset profile ─────────────────────────────────────────────────────────
  async function confirmReset() {
    if (resetting) return;
    resetting = true;
    try {
      await invoke("reset_user_profile");
      values = {};
      sources = {};
      addToast($t("settings.profile.toast.reset_done"), "success");
      resetOpen = false;
      await invoke("trigger_onboarding", { topic: null, profile: null });
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      resetting = false;
    }
  }

  // ── Static option lists ───────────────────────────────────────────────────
  const SECTORS = [
    { v: "", labelKey: "settings.profile.option.none" },
    { v: "fintech", labelKey: "settings.profile.sector.fintech" },
    { v: "sante", labelKey: "settings.profile.sector.sante" },
    { v: "ecommerce", labelKey: "settings.profile.sector.ecommerce" },
    { v: "industrie", labelKey: "settings.profile.sector.industrie" },
    { v: "education", labelKey: "settings.profile.sector.education" },
    { v: "autre", labelKey: "settings.profile.sector.autre" },
  ];
  const TEAM_SIZES = [
    { v: "", labelKey: "settings.profile.option.none" },
    { v: "solo", labelKey: "settings.profile.team_size.solo" },
    { v: "2-5", labelKey: "settings.profile.team_size.2_5" },
    { v: "6-20", labelKey: "settings.profile.team_size.6_20" },
    { v: "20+", labelKey: "settings.profile.team_size.20_plus" },
  ];
  const PROFICIENCY = [
    { v: "", labelKey: "settings.profile.option.none" },
    { v: "debutant", labelKey: "settings.profile.proficiency.debutant" },
    { v: "intermediaire", labelKey: "settings.profile.proficiency.intermediaire" },
    { v: "avance", labelKey: "settings.profile.proficiency.avance" },
    { v: "expert", labelKey: "settings.profile.proficiency.expert" },
  ];
  const LANGUAGES = [
    { v: "", labelKey: "settings.profile.option.none" },
    { v: "fr", labelKey: "settings.profile.language.fr" },
    { v: "en", labelKey: "settings.profile.language.en" },
  ];
  const LLM_BACKENDS = [
    { v: "", labelKey: "settings.profile.option.none" },
    { v: "local", labelKey: "settings.profile.llm.local" },
    { v: "ollama", labelKey: "settings.profile.llm.ollama" },
    { v: "anthropic", labelKey: "settings.profile.llm.anthropic" },
    { v: "openai", labelKey: "settings.profile.llm.openai" },
    { v: "bedrock", labelKey: "settings.profile.llm.bedrock" },
    { v: "vertex", labelKey: "settings.profile.llm.vertex" },
  ];

  const HITL_OPTIONS = [
    {
      v: "always",
      labelKey: "settings.profile.hitl.always.label",
      hintKey: "settings.profile.hitl.always.hint",
    },
    {
      v: "critical-only",
      labelKey: "settings.profile.hitl.critical_only.label",
      hintKey: "settings.profile.hitl.critical_only.hint",
    },
    {
      v: "never",
      labelKey: "settings.profile.hitl.never.label",
      hintKey: "settings.profile.hitl.never.hint",
    },
  ];

  const SOVEREIGNTY_OPTIONS = [
    {
      v: "local-only",
      labelKey: "settings.profile.sovereignty.local_only.label",
      hintKey: "settings.profile.sovereignty.local_only.hint",
    },
    {
      v: "local-preferred",
      labelKey: "settings.profile.sovereignty.local_preferred.label",
      hintKey: "settings.profile.sovereignty.local_preferred.hint",
    },
    {
      v: "cloud-ok",
      labelKey: "settings.profile.sovereignty.cloud_ok.label",
      hintKey: "settings.profile.sovereignty.cloud_ok.hint",
    },
  ];

  const TRIGGER_OPTIONS = [
    { v: "manuel", labelKey: "settings.profile.trigger.manuel" },
    { v: "planifie", labelKey: "settings.profile.trigger.planifie" },
    { v: "evenementiel", labelKey: "settings.profile.trigger.evenementiel" },
  ];

  const COMPLIANCE = ["RGPD", "HIPAA", "SOC2"];
  const INTEGRATIONS = ["GitHub", "Slack", "Notion", "Gmail"];

  // Sensitive label rendered next to a section/field title.
  // The marker explains why the field matters and how to apply changes.
</script>

{#snippet sourceBadge(key: string)}
  {#if sourceLabel(sources[key])}
    <span
      class={sourceBadgeClasses(sources[key])}
      title={$t("settings.profile.source.tooltip", { values: { source: sourceLabel(sources[key]) } })}
    >
      {sourceLabel(sources[key])}
    </span>
  {/if}
{/snippet}

{#if loading}
  <SettingSectionSkeleton />
{:else}
  <section class="space-y-6" data-testid="profile-section">
    <p class="text-xs text-muted-foreground">
      {$t("settings.profile.intro")}
    </p>

    {#if showEnrichmentBanner}
      <div
        class="flex flex-col gap-3 rounded-lg border border-primary/30 bg-primary/5 p-4 sm:flex-row sm:items-center sm:justify-between"
        data-testid="profile-enrichment-banner"
      >
        <div class="flex flex-col gap-0.5">
          <span class="text-sm font-semibold text-foreground">
            {$t("settings.profile.enrichment.title")}
          </span>
          <span class="text-xs text-muted-foreground">
            {$t("settings.profile.enrichment.detail")}
          </span>
        </div>
        <div class="flex flex-shrink-0 items-center gap-2">
          <Button
            variant="primary-gradient"
            size="sm"
            onclick={resumeEnrichment}
            data-testid="profile-enrichment-resume"
          >
            {$t("settings.profile.enrichment.resume")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onclick={() => (enrichmentDismissed = true)}
            data-testid="profile-enrichment-dismiss"
          >
            {$t("settings.profile.enrichment.dismiss")}
          </Button>
        </div>
      </div>
    {/if}

    <!-- ─── Identité ──────────────────────────────────────────────────────── -->
    <Card class="space-y-4 rounded-lg p-4">
      <header class="flex items-center gap-2">
        <span class="flex h-9 w-9 items-center justify-center rounded-full bg-primary/10 text-primary">
          <User size={18} strokeWidth={1.75} />
        </span>
        <div>
          <h3 class="text-sm font-semibold">{$t("settings.profile.identity.title")}</h3>
          <p class="text-[11px] text-muted-foreground">{$t("settings.profile.identity.subtitle")}</p>
        </div>
      </header>

      <div class="grid gap-3 md:grid-cols-2">
        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            {$t("settings.profile.fields.name")}
            {@render sourceBadge("name")}
          </span>
          <Input
            value={val("name")}
            onblur={(e: Event) => saveKey("name", (e.target as HTMLInputElement).value)}
            placeholder={$t("settings.profile.placeholder.name")}
            disabled={saving["name"]}
            data-testid="profile-input-name"
          />
        </label>

        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            {$t("settings.profile.fields.role")}
            {@render sourceBadge("role")}
          </span>
          <Input
            value={val("role")}
            onblur={(e: Event) => saveKey("role", (e.target as HTMLInputElement).value)}
            placeholder={$t("settings.profile.placeholder.role")}
            disabled={saving["role"]}
            data-testid="profile-input-role"
          />
        </label>

        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            {$t("settings.profile.fields.sector")}
            {@render sourceBadge("domain.sector")}
          </span>
          <Select
            value={val("domain.sector")}
            onchange={(e: Event) => saveKey("domain.sector", (e.target as HTMLSelectElement).value)}
            data-testid="profile-select-sector"
          >
            {#each SECTORS as opt}
              <option value={opt.v}>{$t(opt.labelKey)}</option>
            {/each}
          </Select>
        </label>

        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            {$t("settings.profile.fields.team_size")}
            {@render sourceBadge("domain.team_size")}
          </span>
          <Select
            value={val("domain.team_size")}
            onchange={(e: Event) => saveKey("domain.team_size", (e.target as HTMLSelectElement).value)}
            data-testid="profile-select-team-size"
          >
            {#each TEAM_SIZES as opt}
              <option value={opt.v}>{$t(opt.labelKey)}</option>
            {/each}
          </Select>
        </label>
      </div>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.goals")}
          {@render sourceBadge("goals")}
        </span>
        <Textarea
          value={val("goals")}
          onblur={(e: Event) => saveKey("goals", (e.target as HTMLTextAreaElement).value)}
          placeholder={$t("settings.profile.placeholder.goals")}
          rows={2}
          disabled={saving["goals"]}
          data-testid="profile-textarea-goals"
        />
      </label>
    </Card>

    <!-- ─── Supervision Agents (sensible) ─────────────────────────────────── -->
    <Card class="space-y-4 rounded-lg border-warning/30 p-4">
      <header class="flex items-start gap-2">
        <span class="flex h-9 w-9 items-center justify-center rounded-full bg-warning/10 text-warning">
          <ShieldAlert size={18} strokeWidth={1.75} />
        </span>
        <div class="flex-1">
          <h3 class="flex items-center gap-1.5 text-sm font-semibold">
            {$t("settings.profile.supervision.title")}
            <span class="rounded-full bg-warning/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider text-warning">
              {$t("settings.profile.sensitive_badge")}
            </span>
          </h3>
          <p class="text-[11px] text-muted-foreground">
            {$t("settings.profile.sensitive_notice")}
          </p>
        </div>
      </header>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.hitl")}
          {@render sourceBadge("agents.hitl")}
        </span>
        <RadioGroup
          value={val("agents.hitl")}
          onchange={(v: string) => saveKey("agents.hitl", v)}
          data-testid="profile-radio-hitl"
        >
          {#each HITL_OPTIONS as opt}
            <RadioItem
              value={opt.v}
              checked={isSet("agents.hitl", opt.v)}
              onchange={(v: string) => saveKey("agents.hitl", v)}
              loading={saving["agents.hitl"] && isSet("agents.hitl", opt.v)}
            >
              <span class="flex flex-col">
                <span class="text-sm font-medium">{$t(opt.labelKey)}</span>
                <span class="text-[11px] text-muted-foreground">{$t(opt.hintKey)}</span>
              </span>
            </RadioItem>
          {/each}
        </RadioGroup>
      </div>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.agent_domains")}
          {@render sourceBadge("agents.domains")}
        </span>
        <Input
          value={val("agents.domains")}
          onblur={(e: Event) => saveKey("agents.domains", (e.target as HTMLInputElement).value)}
          placeholder={$t("settings.profile.placeholder.agent_domains")}
          disabled={saving["agents.domains"]}
          data-testid="profile-input-agent-domains"
        />
        <span class="block text-[11px] text-muted-foreground">
          {$t("settings.profile.hint.agent_domains")}
        </span>
      </label>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.trigger")}
          {@render sourceBadge("agents.trigger")}
        </span>
        <p class="text-[11px] text-muted-foreground">
          {@html $t("settings.profile.hint.trigger")}
        </p>
        <RadioGroup
          value={val("agents.trigger")}
          onchange={(v: string) => saveKey("agents.trigger", v)}
          class="flex-row flex-wrap gap-4"
          data-testid="profile-radio-trigger"
        >
          {#each TRIGGER_OPTIONS as opt}
            <RadioItem
              value={opt.v}
              checked={isSet("agents.trigger", opt.v)}
              onchange={(v: string) => saveKey("agents.trigger", v)}
            >
              <span class="text-sm">{$t(opt.labelKey)}</span>
            </RadioItem>
          {/each}
        </RadioGroup>
      </div>
    </Card>

    <!-- ─── Outils & contexte métier ──────────────────────────────────────── -->
    <Card class="space-y-4 rounded-lg p-4">
      <header class="flex items-center gap-2">
        <span class="flex h-9 w-9 items-center justify-center rounded-full bg-primary/10 text-primary">
          <Cpu size={18} strokeWidth={1.75} />
        </span>
        <div>
          <h3 class="text-sm font-semibold">{$t("settings.profile.tools.title")}</h3>
          <p class="text-[11px] text-muted-foreground">
            {$t("settings.profile.tools.subtitle")}
          </p>
        </div>
      </header>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.tools_daily")}
          {@render sourceBadge("tools.daily")}
        </span>
        <Input
          value={val("tools.daily")}
          onblur={(e: Event) => saveKey("tools.daily", (e.target as HTMLInputElement).value)}
          placeholder={$t("settings.profile.placeholder.tools_daily")}
          disabled={saving["tools.daily"]}
          data-testid="profile-input-tools-daily"
        />
        <span class="block text-[11px] text-muted-foreground">
          {$t("settings.profile.hint.tools_daily")}
        </span>
      </label>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.tech_stack")}
          {@render sourceBadge("tech.stack")}
        </span>
        <Input
          value={val("tech.stack")}
          onblur={(e: Event) => saveKey("tech.stack", (e.target as HTMLInputElement).value)}
          placeholder={$t("settings.profile.placeholder.tech_stack")}
          disabled={saving["tech.stack"]}
          data-testid="profile-input-tech-stack"
        />
        <span class="block text-[11px] text-muted-foreground">
          {$t("settings.profile.hint.tech_stack")}
        </span>
      </label>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.proficiency")}
          {@render sourceBadge("tech.proficiency")}
        </span>
        <Select
          value={val("tech.proficiency")}
          onchange={(e: Event) => saveKey("tech.proficiency", (e.target as HTMLSelectElement).value)}
          data-testid="profile-select-proficiency"
        >
          {#each PROFICIENCY as opt}
            <option value={opt.v}>{$t(opt.labelKey)}</option>
          {/each}
        </Select>
        <span class="block text-[11px] text-muted-foreground">
          {$t("settings.profile.hint.proficiency")}
        </span>
      </label>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.integrations")}
          {@render sourceBadge("tech.integrations")}
          <span class="rounded-full bg-warning/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider text-warning">
            {$t("settings.profile.sensitive_badge")}
          </span>
        </span>
        <p class="text-[11px] text-muted-foreground">
          {$t("settings.profile.hint.integrations")}
        </p>
        <div class="flex flex-wrap gap-3">
          {#each INTEGRATIONS as integ}
            <label class="flex items-center gap-2 text-sm">
              <Checkbox
                checked={listIncludes("tech.integrations", integ)}
                onchange={(c: boolean) => toggleListEntry("tech.integrations", integ, c)}
                loading={saving["tech.integrations"]}
                data-testid={`profile-integ-${integ.toLowerCase()}`}
              />
              <span>{integ}</span>
            </label>
          {/each}
        </div>
      </div>
    </Card>

    <!-- ─── Contraintes (sensible) ────────────────────────────────────────── -->
    <Card class="space-y-4 rounded-lg border-warning/30 p-4">
      <header class="flex items-start gap-2">
        <span class="flex h-9 w-9 items-center justify-center rounded-full bg-warning/10 text-warning">
          <Briefcase size={18} strokeWidth={1.75} />
        </span>
        <div class="flex-1">
          <h3 class="flex items-center gap-1.5 text-sm font-semibold">
            {$t("settings.profile.constraints.title")}
            <span class="rounded-full bg-warning/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider text-warning">
              {$t("settings.profile.sensitive_badge")}
            </span>
          </h3>
          <p class="text-[11px] text-muted-foreground">
            {$t("settings.profile.sensitive_notice")}
          </p>
        </div>
      </header>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.sovereignty")}
          {@render sourceBadge("constraints.sovereignty")}
        </span>
        <RadioGroup
          value={val("constraints.sovereignty")}
          onchange={(v: string) => saveKey("constraints.sovereignty", v)}
          data-testid="profile-radio-sovereignty"
        >
          {#each SOVEREIGNTY_OPTIONS as opt}
            <RadioItem
              value={opt.v}
              checked={isSet("constraints.sovereignty", opt.v)}
              onchange={(v: string) => saveKey("constraints.sovereignty", v)}
              loading={saving["constraints.sovereignty"] && isSet("constraints.sovereignty", opt.v)}
            >
              <span class="flex flex-col">
                <span class="text-sm font-medium">{$t(opt.labelKey)}</span>
                <span class="text-[11px] text-muted-foreground">{$t(opt.hintKey)}</span>
              </span>
            </RadioItem>
          {/each}
        </RadioGroup>
      </div>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          {$t("settings.profile.fields.compliance")}
          {@render sourceBadge("constraints.compliance")}
        </span>
        <div class="flex flex-wrap gap-3">
          {#each COMPLIANCE as norm}
            <label class="flex items-center gap-2 text-sm">
              <Checkbox
                checked={listIncludes("constraints.compliance", norm)}
                onchange={(c: boolean) => toggleListEntry("constraints.compliance", norm, c)}
                loading={saving["constraints.compliance"]}
                data-testid={`profile-compliance-${norm.toLowerCase()}`}
              />
              <span>{norm}</span>
            </label>
          {/each}
        </div>
      </div>
    </Card>

    <!-- ─── Préférences ───────────────────────────────────────────────────── -->
    <Card class="space-y-4 rounded-lg p-4">
      <header class="flex items-center gap-2">
        <span class="flex h-9 w-9 items-center justify-center rounded-full bg-primary/10 text-primary">
          <Settings2 size={18} strokeWidth={1.75} />
        </span>
        <div>
          <h3 class="text-sm font-semibold">{$t("settings.profile.preferences.title")}</h3>
          <p class="text-[11px] text-muted-foreground">{$t("settings.profile.preferences.subtitle")}</p>
        </div>
      </header>

      <div class="grid gap-3 md:grid-cols-2">
        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            {$t("settings.profile.fields.language")}
            {@render sourceBadge("preferences.language")}
          </span>
          <Select
            value={val("preferences.language")}
            onchange={(e: Event) => saveKey("preferences.language", (e.target as HTMLSelectElement).value)}
            data-testid="profile-select-language"
          >
            {#each LANGUAGES as opt}
              <option value={opt.v}>{$t(opt.labelKey)}</option>
            {/each}
          </Select>
        </label>

        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            {$t("settings.profile.fields.llm")}
            {@render sourceBadge("preferences.llm")}
          </span>
          <Select
            value={val("preferences.llm")}
            onchange={(e: Event) => saveKey("preferences.llm", (e.target as HTMLSelectElement).value)}
            data-testid="profile-select-llm"
          >
            {#each LLM_BACKENDS as opt}
              <option value={opt.v}>{$t(opt.labelKey)}</option>
            {/each}
          </Select>
        </label>
      </div>
    </Card>

    <!-- ─── Zone danger ───────────────────────────────────────────────────── -->
    <div class="rounded-lg border border-destructive/30 bg-destructive/5 p-4 space-y-2">
      <div class="flex items-start gap-2">
        <AlertTriangle size={16} class="text-destructive mt-0.5" />
        <div class="flex-1">
          <h3 class="text-sm font-semibold text-destructive">{$t("settings.profile.danger.title")}</h3>
          <p class="text-[11px] text-muted-foreground">
            {$t("settings.profile.danger.description")}
          </p>
        </div>
      </div>
      <Button
        variant="outline"
        size="sm"
        onclick={() => (resetOpen = true)}
        class="border-destructive/40 text-destructive hover:bg-destructive/10"
        data-testid="profile-reset-button"
      >
        {$t("settings.profile.danger.reset_button")}
      </Button>
    </div>
  </section>
{/if}

<ConfirmDialog
  open={resetOpen}
  onclose={() => (resetOpen = false)}
  onconfirm={confirmReset}
  title={$t("settings.profile.reset_dialog.title")}
  message={$t("settings.profile.reset_dialog.message")}
  confirmLabel={$t("settings.profile.reset_dialog.confirm")}
  cancelLabel={$t("common.cancel")}
  loading={resetting}
  data-testid="profile-reset-confirm"
/>
