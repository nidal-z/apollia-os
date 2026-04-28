<script lang="ts">
  import { X, Sparkles } from "lucide-svelte";
  import BtnPrimary from "./BtnPrimary.svelte";
  import BtnSecondary from "./BtnSecondary.svelte";

  export type ProjectTemplate = {
    id: string;
    name: string;
    description: string;
    /** Recommended agent names. */
    agents: string[];
    /** Accent color CSS. */
    color?: string;
    /** Special "blank" template variant. */
    blank?: boolean;
  };

  interface Props {
    open?: boolean;
    templates: ProjectTemplate[];
    /** Initially selected template id. */
    selectedId?: string;
    /** Two-step flow: 0 = pick template, 1 = name & describe. */
    step?: 0 | 1;
    /** Pre-filled values for step 2. */
    name?: string;
    description?: string;
    color?: string;
    onCancel?: () => void;
    onContinue?: (templateId: string) => void;
    onCreate?: (data: { templateId: string; name: string; description: string; color: string }) => void;
  }

  let {
    open = $bindable(false),
    templates,
    selectedId = $bindable(),
    step = $bindable(0),
    name = $bindable(""),
    description = $bindable(""),
    color = $bindable("hsl(var(--primary))"),
    onCancel,
    onContinue,
    onCreate,
  }: Props = $props();

  const COLOR_SWATCHES = [
    "hsl(var(--primary))",
    "hsl(var(--secondary))",
    "#10b981",
    "#f59e0b",
    "#ef4444",
    "#0ea5e9",
    "#ec4899",
    "#64748b",
  ];

  const selected = $derived(templates.find((t) => t.id === selectedId) ?? templates[0]);

  function close() {
    open = false;
    onCancel?.();
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center"
    style="background: hsl(var(--foreground) / 0.35);"
    role="dialog"
    aria-modal="true"
  >
    {#if step === 0}
      <div
        class="bg-card rounded-2xl p-6 w-[640px] max-w-[92vw] flex flex-col shadow-elev-4"
      >
        <div class="flex items-center justify-between mb-3">
          <div>
            <div
              class="text-[11px] tracking-[1.2px] text-muted-foreground font-mono font-semibold"
            >
              ÉTAPE 1 / 2
            </div>
            <h2
              class="mt-1 m-0 font-normal text-foreground"
              style="font-size: 20px; font-weight: 600; letter-spacing: -0.3px;"
            >
              Partir d'un template ?
            </h2>
          </div>
          <button
            type="button"
            onclick={close}
            class="bg-transparent border-0 text-muted-foreground cursor-pointer hover:text-foreground"
            aria-label="Fermer"
          >
            <X size={16} />
          </button>
        </div>
        <p class="m-0 mb-4 text-[12.5px] text-muted-foreground">
          Les templates pré-configurent agents, connexions et jalons. Vous pouvez tout modifier
          après.
        </p>
        <div class="grid grid-cols-2 gap-2.5">
          {#each templates as t}
            {@const isActive = (selectedId ?? templates[0]?.id) === t.id}
            <button
              type="button"
              onclick={() => (selectedId = t.id)}
              class="px-3.5 py-3 rounded-[10px] cursor-pointer text-left transition-all {isActive
                ? 'bg-primary/10 border-2 border-primary'
                : 'bg-surface-1 border border-border hover:border-primary/40'}"
            >
              <div class="flex items-center gap-1.5 mb-1.5">
                <div
                  class="w-1.5 h-1.5 rounded-full"
                  style="background: {t.color ?? 'hsl(var(--primary))'};"
                ></div>
                <span class="text-[12.5px] font-semibold text-foreground">{t.name}</span>
              </div>
              <div class="text-[10.5px] text-muted-foreground mb-1.5">
                {t.description}
              </div>
              {#if t.agents.length > 0}
                <div class="flex gap-1 flex-wrap">
                  {#each t.agents as a}
                    <span
                      class="text-[9.5px] px-1.5 py-px rounded-full bg-card text-muted-foreground border border-border"
                    >
                      {a}
                    </span>
                  {/each}
                </div>
              {:else if t.blank}
                <div class="text-[10px] text-muted-foreground italic">
                  À configurer manuellement
                </div>
              {/if}
            </button>
          {/each}
        </div>
        <div class="flex gap-2 mt-4">
          <BtnSecondary onclick={close}>Annuler</BtnSecondary>
          <div class="flex-1"></div>
          <BtnPrimary
            onclick={() => {
              const id = selectedId ?? templates[0]?.id;
              if (id) {
                step = 1;
                onContinue?.(id);
              }
            }}
          >
            {#snippet kbd()}<span class="font-mono text-[10px] opacity-80">↵</span>{/snippet}
            Continuer →
          </BtnPrimary>
        </div>
      </div>
    {:else}
      <div
        class="bg-card rounded-2xl p-6 w-[560px] max-w-[92vw] flex flex-col shadow-elev-4"
      >
        <div class="mb-3">
          <div
            class="text-[11px] tracking-[1.2px] text-muted-foreground font-mono font-semibold"
          >
            ÉTAPE 2 / 2 · {selected?.name?.toUpperCase()}
          </div>
          <h2
            class="mt-1 m-0 font-normal text-foreground"
            style="font-size: 20px; font-weight: 600; letter-spacing: -0.3px;"
          >
            Nommez votre projet.
          </h2>
        </div>

        <div class="mb-3.5">
          <div class="text-[11px] text-muted-foreground mb-1 font-semibold">Nom</div>
          <input
            bind:value={name}
            class="w-full px-3 py-2 rounded-lg border-2 border-primary bg-card text-[13.5px] font-medium text-foreground outline-none"
            placeholder="Lancement produit Q2"
          />
        </div>

        <div class="mb-3.5">
          <div class="text-[11px] text-muted-foreground mb-1 font-semibold">
            Description
          </div>
          <textarea
            bind:value={description}
            rows="3"
            class="w-full px-3 py-2 rounded-lg border border-border bg-surface-1 text-[12.5px] text-muted-foreground min-h-[54px] leading-[1.55] outline-none resize-y"
          ></textarea>
        </div>

        <div class="mb-3.5">
          <div class="text-[11px] text-muted-foreground mb-1.5 font-semibold">Couleur</div>
          <div class="flex gap-2">
            {#each COLOR_SWATCHES as c}
              <button
                type="button"
                onclick={() => (color = c)}
                aria-label="Couleur"
                class="w-6 h-6 rounded-full cursor-pointer transition-shadow"
                style="background: {c}; border: 2px solid {color === c
                  ? 'hsl(var(--foreground))'
                  : 'transparent'}; box-shadow: {color === c
                  ? '0 0 0 2px hsl(var(--card)), 0 0 0 3px hsl(var(--foreground))'
                  : 'none'};"
              ></button>
            {/each}
          </div>
        </div>

        <div
          class="px-3 py-2.5 rounded-lg bg-primary/5 border border-primary/10 mb-3"
        >
          <div
            class="text-[11px] font-semibold text-primary mb-1 inline-flex items-center gap-1.5"
          >
            <Sparkles size={10} /> Prêt à démarrer
          </div>
          <div class="text-[11px] text-muted-foreground leading-[1.5]">
            {selected?.agents?.length ?? 0} agents pré-assignés · prêt à être configuré.
          </div>
        </div>

        <div class="flex gap-2">
          <BtnSecondary onclick={() => (step = 0)}>← Retour</BtnSecondary>
          <div class="flex-1"></div>
          <BtnPrimary
            onclick={() => {
              if (selected) {
                onCreate?.({
                  templateId: selected.id,
                  name,
                  description,
                  color,
                });
              }
            }}
          >
            {#snippet kbd()}<span class="font-mono text-[10px] opacity-80">⌘↵</span>{/snippet}
            Créer le projet
          </BtnPrimary>
        </div>
      </div>
    {/if}
  </div>
{/if}
