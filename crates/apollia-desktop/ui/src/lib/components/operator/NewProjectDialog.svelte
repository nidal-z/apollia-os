<script lang="ts">
  import { t } from "svelte-i18n";
  import { X, Sparkles } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";

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
    "hsl(var(--swatch-green))",
    "hsl(var(--swatch-amber))",
    "hsl(var(--swatch-red))",
    "hsl(var(--swatch-sky))",
    "hsl(var(--swatch-pink))",
    "hsl(var(--swatch-slate))",
  ];

  const selected = $derived(templates.find((t) => t.id === selectedId) ?? templates[0]);

  function close() {
    open = false;
    onCancel?.();
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-overlay flex items-center justify-center"
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
              class="text-caption tracking-[1.2px] text-muted-foreground font-mono font-semibold"
            >
              {$t("projects.dialog.step_1_of_2")}
            </div>
            <h2
              class="mt-1 m-0 font-normal text-foreground"
              style="font-size: var(--text-heading-lg); font-weight: 600; letter-spacing: -0.3px;"
            >
              {$t("projects.dialog.step1_title")}
            </h2>
          </div>
          <Button variant="ghost" size="sm"
            type="button"
            onclick={close}
            class="bg-transparent border-0 text-muted-foreground cursor-pointer hover:text-foreground"
            aria-label={$t("common.close")}
            data-testid="new-project-close"
          >
            <X size={16} />
          </Button>
        </div>
        <p class="m-0 mb-4 text-code-sm text-muted-foreground">
          {$t("projects.dialog.step1_subtitle")}
        </p>
        <div class="grid grid-cols-2 gap-2.5">
          {#each templates as tpl}
            {@const isActive = (selectedId ?? templates[0]?.id) === tpl.id}
            <Button variant="ghost" size="auto"
              type="button"
              onclick={() => (selectedId = tpl.id)}
              class="w-full flex-col items-stretch whitespace-normal px-3.5 py-3 rounded-lg cursor-pointer text-left transition-all {isActive
                ? 'bg-primary/10 border-2 border-primary'
                : 'bg-surface-1 border border-border hover:border-primary/40'}"
              data-testid="new-project-template-{tpl.id}"
            >
              <div class="flex items-center gap-1.5 mb-1.5">
                <div
                  class="w-1.5 h-1.5 rounded-full"
                  style="background: {tpl.color ?? 'hsl(var(--primary))'};"
                ></div>
                <span class="text-code-sm font-semibold text-foreground">{tpl.name}</span>
              </div>
              <div class="text-micro-lg text-muted-foreground mb-1.5">
                {tpl.description}
              </div>
              {#if tpl.agents.length > 0}
                <div class="flex gap-1 flex-wrap">
                  {#each tpl.agents as a}
                    <span
                      class="text-micro-sm px-1.5 py-px rounded-full leading-none bg-card text-muted-foreground border border-border"
                    >
                      {a}
                    </span>
                  {/each}
                </div>
              {:else if tpl.blank}
                <div class="text-micro text-muted-foreground italic">
                  {$t("projects.dialog.manual_config")}
                </div>
              {/if}
            </Button>
          {/each}
        </div>
        <div class="flex gap-2 mt-4">
          <Button variant="outline" size="sm" onclick={close} data-testid="new-project-cancel">{$t("common.cancel")}</Button>
          <div class="flex-1"></div>
          <Button variant="primary-solid" size="sm"
            onclick={() => {
              const id = selectedId ?? templates[0]?.id;
              if (id) {
                step = 1;
                onContinue?.(id);
              }
            }}
            data-testid="new-project-continue"
          >
            {#snippet kbd()}<span class="font-mono text-micro opacity-80">↵</span>{/snippet}
            {$t("common.continue")}
          </Button>
        </div>
      </div>
    {:else}
      <div
        class="bg-card rounded-2xl p-6 w-[640px] max-w-[92vw] flex flex-col shadow-elev-4"
      >
        <div class="mb-3">
          <div
            class="text-caption tracking-[1.2px] text-muted-foreground font-mono font-semibold"
          >
            {$t("projects.dialog.step_2_of_2", { values: { name: selected?.name?.toUpperCase() ?? "" } })}
          </div>
          <h2
            class="mt-1 m-0 font-normal text-foreground"
            style="font-size: var(--text-heading-lg); font-weight: 600; letter-spacing: -0.3px;"
          >
            {$t("projects.dialog.step2_title")}
          </h2>
        </div>

        <div class="mb-3.5">
          <label
            for="new-project-name"
            class="block text-caption text-muted-foreground mb-1 font-semibold"
          >{$t("projects.field_name")}</label>
          <Input
            id="new-project-name"
            bind:value={name}
            class="w-full px-3 py-2 rounded-lg border-2 border-primary bg-card text-body-sm font-medium text-foreground outline-none"
            placeholder={$t("projects.dialog.name_placeholder")}
            data-testid="new-project-name-input"
           />
        </div>

        <div class="mb-3.5">
          <label
            for="new-project-description"
            class="block text-caption text-muted-foreground mb-1 font-semibold"
          >
            {$t("projects.field_description")}
          </label>
          <Textarea
            id="new-project-description"
            bind:value={description}
            rows={3}
            class="w-full px-3 py-2 rounded-lg border border-border bg-surface-1 text-code-sm text-muted-foreground min-h-[54px] leading-[1.55] outline-none resize-y"
            data-testid="new-project-description-input"
          ></Textarea>
        </div>

        <div class="mb-3.5">
          <div class="text-caption text-muted-foreground mb-1.5 font-semibold">{$t("projects.dialog.color_label")}</div>
          <div class="flex gap-2">
            {#each COLOR_SWATCHES as c}
              <Button variant="ghost" size="sm"
                type="button"
                onclick={() => (color = c)}
                aria-label={$t("projects.dialog.color_label")}
                data-testid="new-project-color-{c}"
                class="w-6 h-6 rounded-full cursor-pointer transition-shadow"
                style="background: {c}; border: 2px solid {color === c
                  ? 'hsl(var(--foreground))'
                  : 'transparent'}; box-shadow: {color === c
                  ? '0 0 0 2px hsl(var(--card)), 0 0 0 3px hsl(var(--foreground))'
                  : 'none'};"
              ></Button>
            {/each}
          </div>
        </div>

        <div
          class="px-3 py-2.5 rounded-lg bg-primary/5 border border-primary/10 mb-3"
        >
          <div
            class="text-caption font-semibold text-primary mb-1 inline-flex items-center gap-1.5"
          >
            <Sparkles size={10} /> {$t("projects.dialog.ready_title")}
          </div>
          <div class="text-caption text-muted-foreground leading-[1.5]">
            {$t("projects.dialog.ready_detail", { values: { count: selected?.agents?.length ?? 0 } })}
          </div>
        </div>

        <div class="flex gap-2">
          <Button variant="outline" size="sm" onclick={() => (step = 0)} data-testid="new-project-back">{$t("common.back")}</Button>
          <div class="flex-1"></div>
          <Button variant="primary-solid" size="sm"
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
            data-testid="new-project-create"
          >
            {#snippet kbd()}<span class="font-mono text-micro opacity-80">⌘↵</span>{/snippet}
            {$t("projects.create_project")}
          </Button>
        </div>
      </div>
    {/if}
  </div>
{/if}
