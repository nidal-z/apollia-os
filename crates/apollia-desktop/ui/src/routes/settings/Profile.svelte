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
        addToast(
          "Profil mis à jour. Relancez l'onboarding pour adapter vos règles de permissions à ce changement.",
          "info",
        );
      } else {
        addToast("Profil mis à jour", "success");
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
    if (src === "onboarding") return "onboarding";
    if (src === "user") return "vous";
    if (src.startsWith("agent:")) return "agent";
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
      addToast("Profil réinitialisé", "success");
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
    { v: "", label: "-" },
    { v: "fintech", label: "Fintech" },
    { v: "sante", label: "Santé" },
    { v: "ecommerce", label: "E-commerce" },
    { v: "industrie", label: "Industrie" },
    { v: "education", label: "Éducation" },
    { v: "autre", label: "Autre" },
  ];
  const TEAM_SIZES = [
    { v: "", label: "-" },
    { v: "solo", label: "Solo" },
    { v: "2-5", label: "2–5" },
    { v: "6-20", label: "6–20" },
    { v: "20+", label: "20+" },
  ];
  const PROFICIENCY = [
    { v: "", label: "-" },
    { v: "debutant", label: "Débutant" },
    { v: "a-laise", label: "À l'aise" },
    { v: "expert", label: "Expert" },
  ];
  const LANGUAGES = [
    { v: "", label: "-" },
    { v: "fr", label: "Français" },
    { v: "en", label: "English" },
  ];
  const LLM_BACKENDS = [
    { v: "", label: "-" },
    { v: "local", label: "Local (llama.cpp)" },
    { v: "ollama", label: "Ollama" },
    { v: "anthropic", label: "Anthropic" },
    { v: "openai", label: "OpenAI" },
    { v: "bedrock", label: "AWS Bedrock" },
    { v: "vertex", label: "Vertex AI" },
  ];

  const HITL_OPTIONS = [
    {
      v: "always",
      label: "Toujours valider",
      hint: "Chaque action sensible nécessite ton accord explicite.",
    },
    {
      v: "critical-only",
      label: "Critique seulement",
      hint: "Validation requise uniquement pour les actions à fort impact (paiement, suppression…).",
    },
    {
      v: "never",
      label: "Jamais",
      hint: "Les agents agissent sans confirmation. Recommandé uniquement pour des outils en lecture seule.",
    },
  ];

  const SOVEREIGNTY_OPTIONS = [
    {
      v: "local-only",
      label: "Local strict",
      hint: "Aucune donnée ne quitte la machine. LLM cloud désactivé.",
    },
    {
      v: "local-preferred",
      label: "Local préféré",
      hint: "Local par défaut, cloud autorisé en dernier recours après accord.",
    },
    {
      v: "cloud-ok",
      label: "Cloud autorisé",
      hint: "Les agents peuvent utiliser des LLM cloud sans demander.",
    },
  ];

  const TRIGGER_OPTIONS = [
    { v: "manuel", label: "Manuel" },
    { v: "planifie", label: "Planifié" },
    { v: "evenementiel", label: "Événementiel" },
  ];

  const COMPLIANCE = ["RGPD", "HIPAA", "SOC2"];
  const INTEGRATIONS = ["GitHub", "Slack", "Notion", "Gmail"];

  // Sensitive label rendered next to a section/field title.
  // The marker explains why the field matters and how to apply changes.
</script>

{#snippet sourceBadge(key: string)}
  {#if sourceLabel(sources[key])}
    <span class={sourceBadgeClasses(sources[key])} title={`Source : ${sourceLabel(sources[key])}`}>
      {sourceLabel(sources[key])}
    </span>
  {/if}
{/snippet}

{#if loading}
  <SettingSectionSkeleton />
{:else}
  <section class="space-y-6" data-testid="profile-section">
    <p class="text-xs text-muted-foreground">
      Enrichis ton profil progressivement. Plus Apollia te connaît, plus elle peut te
      proposer des automatisations pertinentes. Aucune donnée ne quitte ta machine.
    </p>

    <!-- ─── Identité ──────────────────────────────────────────────────────── -->
    <Card class="space-y-4 rounded-lg p-4">
      <header class="flex items-center gap-2">
        <span class="flex h-9 w-9 items-center justify-center rounded-full bg-primary/10 text-primary">
          <User size={18} strokeWidth={1.75} />
        </span>
        <div>
          <h3 class="text-sm font-semibold">Identité</h3>
          <p class="text-[11px] text-muted-foreground">Qui tu es et ce que tu cherches à faire.</p>
        </div>
      </header>

      <div class="grid gap-3 md:grid-cols-2">
        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            Prénom / alias
            {@render sourceBadge("name")}
          </span>
          <Input
            value={val("name")}
            onblur={(e: Event) => saveKey("name", (e.target as HTMLInputElement).value)}
            placeholder="Nidal"
            disabled={saving["name"]}
            data-testid="profile-input-name"
          />
        </label>

        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            Rôle
            {@render sourceBadge("role")}
          </span>
          <Input
            value={val("role")}
            onblur={(e: Event) => saveKey("role", (e.target as HTMLInputElement).value)}
            placeholder="CTO, dev fullstack, ops…"
            disabled={saving["role"]}
            data-testid="profile-input-role"
          />
        </label>

        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            Secteur
            {@render sourceBadge("domain.sector")}
          </span>
          <Select
            value={val("domain.sector")}
            onchange={(e: Event) => saveKey("domain.sector", (e.target as HTMLSelectElement).value)}
            data-testid="profile-select-sector"
          >
            {#each SECTORS as opt}
              <option value={opt.v}>{opt.label}</option>
            {/each}
          </Select>
        </label>

        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            Taille d'équipe
            {@render sourceBadge("domain.team_size")}
          </span>
          <Select
            value={val("domain.team_size")}
            onchange={(e: Event) => saveKey("domain.team_size", (e.target as HTMLSelectElement).value)}
            data-testid="profile-select-team-size"
          >
            {#each TEAM_SIZES as opt}
              <option value={opt.v}>{opt.label}</option>
            {/each}
          </Select>
        </label>
      </div>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Objectifs (max 2 phrases)
          {@render sourceBadge("goals")}
        </span>
        <Textarea
          value={val("goals")}
          onblur={(e: Event) => saveKey("goals", (e.target as HTMLTextAreaElement).value)}
          placeholder="Ex. Automatiser ma veille concurrentielle et pré-traiter mes emails."
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
            Supervision des agents
            <span class="rounded-full bg-warning/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider text-warning">
              Sensible
            </span>
          </h3>
          <p class="text-[11px] text-muted-foreground">
            Modifier ces choix n'ajuste pas tes règles de permissions automatiquement -
            relance l'onboarding pour que l'agent te propose les ajustements correspondants.
          </p>
        </div>
      </header>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Niveau HITL (Human-in-the-Loop)
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
                <span class="text-sm font-medium">{opt.label}</span>
                <span class="text-[11px] text-muted-foreground">{opt.hint}</span>
              </span>
            </RadioItem>
          {/each}
        </RadioGroup>
      </div>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Domaines où vos agents travaillent en autonomie
          {@render sourceBadge("agents.domains")}
        </span>
        <Input
          value={val("agents.domains")}
          onblur={(e: Event) => saveKey("agents.domains", (e.target as HTMLInputElement).value)}
          placeholder="veille, email, reporting"
          disabled={saving["agents.domains"]}
          data-testid="profile-input-agent-domains"
        />
        <span class="block text-[11px] text-muted-foreground">
          Liste libre des domaines (veille, email, reporting…) où tu laisses tes agents agir
          sans validation manuelle, sous réserve du niveau de supervision ci-dessus.
        </span>
      </label>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Comment vos agents démarrent par défaut
          {@render sourceBadge("agents.trigger")}
        </span>
        <p class="text-[11px] text-muted-foreground">
          Distinct du niveau de supervision : ceci définit <em>quand</em> un agent s'exécute,
          pas <em>si</em> il a besoin de ton approbation.
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
              <span class="text-sm">{opt.label}</span>
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
          <h3 class="text-sm font-semibold">Outils & contexte métier</h3>
          <p class="text-[11px] text-muted-foreground">
            Aide les agents à parler ton langage et à utiliser les outils que tu connais déjà.
          </p>
        </div>
      </header>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Outils du quotidien (séparés par virgules)
          {@render sourceBadge("tools.daily")}
        </span>
        <Input
          value={val("tools.daily")}
          onblur={(e: Event) => saveKey("tools.daily", (e.target as HTMLInputElement).value)}
          placeholder="Excel, Notion, Salesforce, Gmail, VS Code…"
          disabled={saving["tools.daily"]}
          data-testid="profile-input-tools-daily"
        />
        <span class="block text-[11px] text-muted-foreground">
          Liste libre - applications métier, IDE, outils de bureau, plateformes SaaS.
        </span>
      </label>

      <label class="space-y-1 block">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Aisance avec l'outillage tech
          {@render sourceBadge("tech.proficiency")}
        </span>
        <Select
          value={val("tech.proficiency")}
          onchange={(e: Event) => saveKey("tech.proficiency", (e.target as HTMLSelectElement).value)}
          data-testid="profile-select-proficiency"
        >
          {#each PROFICIENCY as opt}
            <option value={opt.v}>{opt.label}</option>
          {/each}
        </Select>
        <span class="block text-[11px] text-muted-foreground">
          Sert aux agents à doser le niveau d'explication technique (vocabulaire, détail des étapes).
        </span>
      </label>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Connecter Apollia à
          {@render sourceBadge("tech.integrations")}
          <span class="rounded-full bg-warning/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider text-warning">
            Sensible
          </span>
        </span>
        <p class="text-[11px] text-muted-foreground">
          Activer une intégration ouvre des permissions par défaut sur les API associées.
          Relance l'onboarding pour adapter les règles correspondantes.
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
            Contraintes
            <span class="rounded-full bg-warning/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider text-warning">
              Sensible
            </span>
          </h3>
          <p class="text-[11px] text-muted-foreground">
            Modifier ces choix n'ajuste pas tes règles de permissions automatiquement -
            relance l'onboarding pour que l'agent te propose les ajustements correspondants.
          </p>
        </div>
      </header>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Souveraineté des données
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
                <span class="text-sm font-medium">{opt.label}</span>
                <span class="text-[11px] text-muted-foreground">{opt.hint}</span>
              </span>
            </RadioItem>
          {/each}
        </RadioGroup>
      </div>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Conformité
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
          <h3 class="text-sm font-semibold">Préférences</h3>
          <p class="text-[11px] text-muted-foreground">Langue et backend par défaut pour les agents.</p>
        </div>
      </header>

      <div class="grid gap-3 md:grid-cols-2">
        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            Langue des agents
            {@render sourceBadge("preferences.language")}
          </span>
          <Select
            value={val("preferences.language")}
            onchange={(e: Event) => saveKey("preferences.language", (e.target as HTMLSelectElement).value)}
            data-testid="profile-select-language"
          >
            {#each LANGUAGES as opt}
              <option value={opt.v}>{opt.label}</option>
            {/each}
          </Select>
        </label>

        <label class="space-y-1">
          <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            Backend LLM préféré
            {@render sourceBadge("preferences.llm")}
          </span>
          <Select
            value={val("preferences.llm")}
            onchange={(e: Event) => saveKey("preferences.llm", (e.target as HTMLSelectElement).value)}
            data-testid="profile-select-llm"
          >
            {#each LLM_BACKENDS as opt}
              <option value={opt.v}>{opt.label}</option>
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
          <h3 class="text-sm font-semibold text-destructive">Zone danger</h3>
          <p class="text-[11px] text-muted-foreground">
            Réinitialiser ton profil supprime toute la mémoire utilisateur et relance
            l'onboarding depuis zéro.
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
        Réinitialiser le profil
      </Button>
    </div>
  </section>
{/if}

<ConfirmDialog
  open={resetOpen}
  onclose={() => (resetOpen = false)}
  onconfirm={confirmReset}
  title="Réinitialiser le profil ?"
  message="Cela supprimera tout le profil mémorisé et relancera l'onboarding. Cette action est irréversible."
  confirmLabel="Réinitialiser"
  cancelLabel="Annuler"
  loading={resetting}
  data-testid="profile-reset-confirm"
/>
