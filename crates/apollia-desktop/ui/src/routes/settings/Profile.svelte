<script lang="ts" context="module">
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
  // Profil keys — mapping clé mémoire → catégorie de stockage
  // (Tier 2 enrichissement progressif, voir docs onboarding ADR-086.)
  // ---------------------------------------------------------------------------
  type Category = "preferences" | "habits" | "context";

  const KEY_CATEGORY: Record<string, Category> = {
    "name": "context",
    "role": "context",
    "goals": "context",
    "domain.sector": "context",
    "domain.team_size": "context",
    "tech.expertise": "context",
    "tech.languages": "context",
    "tech.stack": "context",
    "tech.integrations": "habits",
    "agents.hitl": "habits",
    "agents.domains": "habits",
    "agents.trigger": "habits",
    "constraints.sovereignty": "preferences",
    "constraints.compliance": "preferences",
    "preferences.language": "preferences",
    "preferences.llm": "preferences",
  };

  // Clés sensibles : impactent le comportement d'agents.
  // ADR-086 : la modification ne déclenche PAS de re-dérivation auto ;
  // l'utilisateur doit relancer l'onboarding pour que les règles soient ajustées.
  const SENSITIVE_KEYS = new Set<string>([
    "constraints.sovereignty",
    "constraints.compliance",
    "agents.hitl",
    "tech.integrations",
  ]);

  interface UserMemoryEntryView {
    category: string;
    key: string;
    value: string;
    source: string;
    confidence: number;
    created_at: string;
    updated_at: string;
  }

  interface UserMemoryProfileView {
    entries: UserMemoryEntryView[];
    stats: unknown;
  }

  let loading = $state(true);
  let saving = $state<Record<string, boolean>>({});
  let values = $state<Record<string, string>>({});
  let resetOpen = $state(false);
  let resetting = $state(false);

  // ── Loading ───────────────────────────────────────────────────────────────
  onMount(async () => {
    try {
      const profile = await invoke<UserMemoryProfileView>("get_user_memory_profile");
      const map: Record<string, string> = {};
      for (const e of profile.entries ?? []) {
        map[e.key] = e.value;
      }
      values = map;
    } catch {
      // Mémoire non initialisée → champs vides, pas une erreur.
      values = {};
    } finally {
      loading = false;
    }
  });

  // ── Persistence ───────────────────────────────────────────────────────────
  async function saveKey(key: string, value: string) {
    const category = KEY_CATEGORY[key];
    if (!category) return;
    saving = { ...saving, [key]: true };
    try {
      // Empty string → forget the entry instead of storing an empty value.
      if (value.trim() === "") {
        try {
          await invoke("delete_user_memory_entry", { key });
        } catch {
          // Si l'entrée n'existait pas, ignorer NOT_FOUND.
        }
        delete values[key];
        values = { ...values };
      } else {
        await invoke("update_user_memory_entry", {
          request: { category, key, value: value.trim() },
        });
        values = { ...values, [key]: value.trim() };
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

  // ── Reset profile ─────────────────────────────────────────────────────────
  async function confirmReset() {
    if (resetting) return;
    resetting = true;
    try {
      await invoke("clear_user_memory");
      values = {};
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
    { v: "", label: "—" },
    { v: "fintech", label: "Fintech" },
    { v: "sante", label: "Santé" },
    { v: "ecommerce", label: "E-commerce" },
    { v: "industrie", label: "Industrie" },
    { v: "education", label: "Éducation" },
    { v: "autre", label: "Autre" },
  ];
  const TEAM_SIZES = [
    { v: "", label: "—" },
    { v: "solo", label: "Solo" },
    { v: "2-5", label: "2–5" },
    { v: "6-20", label: "6–20" },
    { v: "20+", label: "20+" },
  ];
  const EXPERTISE = [
    { v: "", label: "—" },
    { v: "debutant", label: "Débutant" },
    { v: "intermediaire", label: "Intermédiaire" },
    { v: "expert", label: "Expert" },
  ];
  const LANGUAGES = [
    { v: "", label: "—" },
    { v: "fr", label: "Français" },
    { v: "en", label: "English" },
  ];
  const LLM_BACKENDS = [
    { v: "", label: "—" },
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
      v: "local-strict",
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

{#if loading}
  <SettingSectionSkeleton />
{:else}
  <section class="space-y-6" data-testid="profile-section">
    <p class="text-xs text-muted-foreground">
      Enrichis ton profil progressivement. Plus Apollia te connaît, plus elle peut te
      proposer des automatisations pertinentes. Aucune donnée ne quitte ta machine.
    </p>

    <!-- ─── Identité ──────────────────────────────────────────────────────── -->
    <div class="glass-card glass-border space-y-4 rounded-lg p-4">
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
          <span class="text-xs font-medium text-muted-foreground">Prénom / alias</span>
          <Input
            value={val("name")}
            onblur={(e: Event) => saveKey("name", (e.target as HTMLInputElement).value)}
            placeholder="Nidal"
            disabled={saving["name"]}
            data-testid="profile-input-name"
          />
        </label>

        <label class="space-y-1">
          <span class="text-xs font-medium text-muted-foreground">Rôle</span>
          <Input
            value={val("role")}
            onblur={(e: Event) => saveKey("role", (e.target as HTMLInputElement).value)}
            placeholder="CTO, dev fullstack, ops…"
            disabled={saving["role"]}
            data-testid="profile-input-role"
          />
        </label>

        <label class="space-y-1">
          <span class="text-xs font-medium text-muted-foreground">Secteur</span>
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
          <span class="text-xs font-medium text-muted-foreground">Taille d'équipe</span>
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
        <span class="text-xs font-medium text-muted-foreground">Objectifs (max 2 phrases)</span>
        <Textarea
          value={val("goals")}
          onblur={(e: Event) => saveKey("goals", (e.target as HTMLTextAreaElement).value)}
          placeholder="Ex. Automatiser ma veille concurrentielle et pré-traiter mes emails."
          rows={2}
          disabled={saving["goals"]}
          data-testid="profile-textarea-goals"
        />
      </label>
    </div>

    <!-- ─── Supervision Agents (sensible) ─────────────────────────────────── -->
    <div class="glass-card glass-border space-y-4 rounded-lg border-warning/30 p-4">
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
            Modifier ces choix n'ajuste pas tes règles de permissions automatiquement —
            relance l'onboarding pour que l'agent te propose les ajustements correspondants.
          </p>
        </div>
      </header>

      <div class="space-y-2">
        <span class="text-xs font-medium text-muted-foreground">Niveau HITL (Human-in-the-Loop)</span>
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
        <span class="text-xs font-medium text-muted-foreground">Domaines auto (séparés par virgules)</span>
        <Input
          value={val("agents.domains")}
          onblur={(e: Event) => saveKey("agents.domains", (e.target as HTMLInputElement).value)}
          placeholder="veille, email, reporting"
          disabled={saving["agents.domains"]}
          data-testid="profile-input-agent-domains"
        />
      </label>

      <div class="space-y-2">
        <span class="text-xs font-medium text-muted-foreground">Mode de déclenchement</span>
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
    </div>

    <!-- ─── Stack technique ───────────────────────────────────────────────── -->
    <div class="glass-card glass-border space-y-4 rounded-lg p-4">
      <header class="flex items-center gap-2">
        <span class="flex h-9 w-9 items-center justify-center rounded-full bg-primary/10 text-primary">
          <Cpu size={18} strokeWidth={1.75} />
        </span>
        <div>
          <h3 class="text-sm font-semibold">Stack technique</h3>
          <p class="text-[11px] text-muted-foreground">Utilisé pour adapter le ton et le détail des explications.</p>
        </div>
      </header>

      <label class="space-y-1 block">
        <span class="text-xs font-medium text-muted-foreground">Niveau d'expertise</span>
        <Select
          value={val("tech.expertise")}
          onchange={(e: Event) => saveKey("tech.expertise", (e.target as HTMLSelectElement).value)}
          data-testid="profile-select-expertise"
        >
          {#each EXPERTISE as opt}
            <option value={opt.v}>{opt.label}</option>
          {/each}
        </Select>
      </label>

      <label class="space-y-1 block">
        <span class="text-xs font-medium text-muted-foreground">Langages (séparés par virgules)</span>
        <Input
          value={val("tech.languages")}
          onblur={(e: Event) => saveKey("tech.languages", (e.target as HTMLInputElement).value)}
          placeholder="rust, python, typescript"
          disabled={saving["tech.languages"]}
          data-testid="profile-input-languages"
        />
      </label>

      <label class="space-y-1 block">
        <span class="text-xs font-medium text-muted-foreground">Stack / outils (séparés par virgules)</span>
        <Input
          value={val("tech.stack")}
          onblur={(e: Event) => saveKey("tech.stack", (e.target as HTMLInputElement).value)}
          placeholder="tokio, svelte, postgres"
          disabled={saving["tech.stack"]}
          data-testid="profile-input-stack"
        />
      </label>

      <div class="space-y-2">
        <span class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
          Intégrations
          <span class="rounded-full bg-warning/15 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider text-warning">
            Sensible
          </span>
        </span>
        <p class="text-[11px] text-muted-foreground">
          Active une intégration ouvre des permissions par défaut. Relance l'onboarding
          pour adapter les règles correspondantes.
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
    </div>

    <!-- ─── Contraintes (sensible) ────────────────────────────────────────── -->
    <div class="glass-card glass-border space-y-4 rounded-lg border-warning/30 p-4">
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
            Modifier ces choix n'ajuste pas tes règles de permissions automatiquement —
            relance l'onboarding pour que l'agent te propose les ajustements correspondants.
          </p>
        </div>
      </header>

      <div class="space-y-2">
        <span class="text-xs font-medium text-muted-foreground">Souveraineté des données</span>
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
        <span class="text-xs font-medium text-muted-foreground">Conformité</span>
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
    </div>

    <!-- ─── Préférences ───────────────────────────────────────────────────── -->
    <div class="glass-card glass-border space-y-4 rounded-lg p-4">
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
          <span class="text-xs font-medium text-muted-foreground">Langue des agents</span>
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
          <span class="text-xs font-medium text-muted-foreground">Backend LLM préféré</span>
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
    </div>

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
