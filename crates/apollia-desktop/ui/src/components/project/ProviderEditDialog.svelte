<script lang="ts">
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    GitBranch,
    FolderTree,
    ScrollText,
    Terminal,
    Palette,
    Folder,
    type Icon,
  } from "lucide-svelte";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Input } from "$lib/components/ui/input";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";
  import type { ProjectProviderRow } from "$lib/types";
  import { setProjectProvider } from "$lib/ipc/projects";

  type ProviderType = "git" | "tree" | "rules" | "script" | "style";

  interface Props {
    open: boolean;
    projectId: string;
    /** When set, the dialog is in edit mode and pre-fills from this row. */
    editing?: ProjectProviderRow | null;
    onclose: () => void;
    /** Called after a successful save (set_project_provider). */
    onsaved?: () => void;
  }

  let { open, projectId, editing = null, onclose, onsaved }: Props = $props();

  const TYPES: Array<{ id: ProviderType; icon: typeof Icon }> = [
    { id: "git", icon: GitBranch },
    { id: "tree", icon: FolderTree },
    { id: "rules", icon: ScrollText },
    { id: "script", icon: Terminal },
    { id: "style", icon: Palette },
  ];

  // ── Local form state ────────────────────────────────────────────────────────
  // Numeric inputs use string state to play nicely with `<Input>` (whose
  // `bind:value` is typed `string`). Values are parsed on save.
  // All fields are populated by the `$effect` below from the `editing` prop.
  let step = $state<"pick" | "form">("pick");
  let providerType = $state<ProviderType>("git");
  let name = $state("");
  let priority = $state<string>("50");
  let enabled = $state(true);
  let path = $state("");
  let maxLines = $state<string>("100");
  let timeoutMs = $state<string>("500");
  let saving = $state(false);

  // ── Re-init when the dialog reopens for a different target ──────────────────
  let lastEditingId: string | null = null;
  let lastOpen = false;
  $effect(() => {
    const editingId = editing?.id ?? null;
    if (open && (!lastOpen || editingId !== lastEditingId)) {
      step = editing ? "form" : "pick";
      providerType = (editing?.provider_type as ProviderType) ?? "git";
      name = editing?.name ?? "";
      priority = String(editing?.priority ?? 50);
      enabled = editing?.enabled ?? true;
      path = editing?.path ?? "";
      maxLines = "100";
      timeoutMs = "500";
      if (editing?.config_json && editing.config_json !== "{}") {
        try {
          const cfg = JSON.parse(editing.config_json) as Record<string, unknown>;
          if (typeof cfg.max_lines === "number") maxLines = String(cfg.max_lines);
          if (typeof cfg.timeout_ms === "number") timeoutMs = String(cfg.timeout_ms);
        } catch {
          // keep defaults
        }
      }
      lastEditingId = editingId;
    }
    lastOpen = open;
  });

  // ── Defaults helpers ────────────────────────────────────────────────────────
  function defaultNameFor(type: ProviderType): string {
    return $t(`projects.provider_type_${type}`);
  }

  function defaultPriorityFor(type: ProviderType): number {
    const map: Record<ProviderType, number> = {
      git: 10,
      tree: 20,
      rules: 30,
      script: 50,
      style: 60,
    };
    return map[type];
  }

  function selectType(t: ProviderType): void {
    providerType = t;
    if (!editing) {
      name = defaultNameFor(t);
      priority = String(defaultPriorityFor(t));
    }
    step = "form";
  }

  async function pickScriptPath(): Promise<void> {
    try {
      const selected = await openDialog({ multiple: false, directory: false });
      if (typeof selected === "string") {
        path = selected;
      }
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  function buildConfigJson(): string {
    switch (providerType) {
      case "tree": {
        const n = parseInt(maxLines, 10);
        return JSON.stringify({ max_lines: Number.isFinite(n) && n > 0 ? n : 100 });
      }
      case "script": {
        const n = parseInt(timeoutMs, 10);
        return JSON.stringify({ timeout_ms: Number.isFinite(n) && n > 0 ? n : 500 });
      }
      case "git":
      case "rules":
      case "style":
      default:
        return "{}";
    }
  }

  const canSave = $derived.by(() => {
    if (!name.trim()) return false;
    if (providerType === "script" && !path.trim()) return false;
    return true;
  });

  async function handleSave(): Promise<void> {
    if (!canSave) return;
    saving = true;
    try {
      const p = parseInt(priority, 10);
      const prio = Number.isFinite(p) ? Math.min(100, Math.max(1, p)) : 50;
      await setProjectProvider({
        providerId: editing?.id ?? null,
        projectId,
        providerType,
        name: name.trim(),
        configJson: buildConfigJson(),
        path: providerType === "script" ? path.trim() : null,
        enabled,
        priority: prio,
      });
      addToast($t("projects.provider_saved_toast"), "success");
      onsaved?.();
      onclose();
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      saving = false;
    }
  }

  const dialogTitle = $derived(
    editing ? $t("projects.provider_edit_title") : $t("projects.provider_create_title"),
  );
</script>

<Dialog {open} {onclose} size="md" title={dialogTitle} data-testid="provider-edit-dialog">
  {#if step === "pick"}
    <p class="m-0 mb-4 text-code-sm text-muted-foreground">
      {$t("projects.provider_pick_subtitle")}
    </p>
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
      {#each TYPES as type (type.id)}
        {@const IconCmp = type.icon}
        <Button variant="ghost" size="auto"
          type="button"
          onclick={() => selectType(type.id)}
          class="flex w-full items-start justify-start whitespace-normal gap-3 px-3.5 py-3 rounded-lg cursor-pointer text-left bg-surface-1 border border-border hover:border-primary/40 transition-colors"
          data-testid="provider-type-{type.id}"
        >
          <div class="shrink-0 w-8 h-8 rounded-md inline-flex items-center justify-center bg-primary/10 text-primary">
            <IconCmp size={14} />
          </div>
          <div class="min-w-0">
            <div class="text-code-sm font-semibold text-foreground">
              {$t(`projects.provider_type_${type.id}`)}
            </div>
            <div class="text-micro-lg text-muted-foreground mt-0.5 leading-[1.45]">
              {$t(`projects.provider_type_${type.id}_desc`)}
            </div>
          </div>
        </Button>
      {/each}
    </div>
    <DialogFooter>
      <Button variant="outline" size="sm" onclick={onclose}>{$t("common.cancel")}</Button>
    </DialogFooter>
  {:else}
    <div class="space-y-4">
      <div>
        <div class="text-caption text-muted-foreground mb-1 font-semibold">
          {$t("projects.provider_field_type")}
        </div>
        <div class="text-code-sm text-foreground">
          {$t(`projects.provider_type_${providerType}`)}
          <span class="text-muted-foreground"> - {$t(`projects.provider_type_${providerType}_desc`)}</span>
        </div>
      </div>

      <div>
        <label
          for="provider-field-name"
          class="block text-caption text-muted-foreground mb-1 font-semibold"
        >
          {$t("projects.provider_field_name")}
        </label>
        <Input id="provider-field-name" bind:value={name} placeholder={defaultNameFor(providerType)} />
      </div>

      {#if providerType === "tree"}
        <div>
          <label
            for="provider-field-max-lines"
            class="block text-caption text-muted-foreground mb-1 font-semibold"
          >
            {$t("projects.provider_field_max_lines")}
          </label>
          <Input id="provider-field-max-lines" type="number" min="1" max="2000" bind:value={maxLines} />
        </div>
      {/if}

      {#if providerType === "script"}
        <div>
          <label
            for="provider-field-script-path"
            class="block text-caption text-muted-foreground mb-1 font-semibold"
          >
            {$t("projects.provider_field_script_path")}
          </label>
          <div class="flex gap-2">
            <Input id="provider-field-script-path" bind:value={path} placeholder="/path/to/script.sh" class="flex-1" />
            <Button variant="outline" size="sm" onclick={pickScriptPath}>
              {#snippet icon()}<Folder size={12} />{/snippet}
              {$t("projects.provider_pick_script")}
            </Button>
          </div>
        </div>
        <div>
          <label
            for="provider-field-timeout-ms"
            class="block text-caption text-muted-foreground mb-1 font-semibold"
          >
            {$t("projects.provider_field_timeout_ms")}
          </label>
          <Input id="provider-field-timeout-ms" type="number" min="50" max="60000" bind:value={timeoutMs} />
        </div>
      {/if}

      {#if providerType === "style"}
        <div class="px-3 py-2 rounded-md bg-warning/10 text-caption text-warning-a11y border border-warning/20">
          {$t("projects.provider_type_style_warning")}
        </div>
      {/if}

      <div class="flex items-center gap-4">
        <div class="flex-1">
          <label
            for="provider-field-priority"
            class="block text-caption text-muted-foreground mb-1 font-semibold"
          >
            {$t("projects.context_priority_label")}
            <span class="text-muted-foreground/70 font-normal"> · {$t("projects.provider_priority_hint")}</span>
          </label>
          <Input id="provider-field-priority" type="number" min="1" max="100" bind:value={priority} />
        </div>
        <div>
          <div class="text-caption text-muted-foreground mb-1 font-semibold">
            {$t("projects.context_enabled_label")}
          </div>
          <Toggle bind:checked={enabled} aria-label={$t("projects.context_enabled_label")} />
        </div>
      </div>
    </div>

    <DialogFooter>
      {#if !editing}
        <Button variant="outline" size="sm" onclick={() => (step = "pick")}>{$t("common.back")}</Button>
      {:else}
        <Button variant="outline" size="sm" onclick={onclose}>{$t("common.cancel")}</Button>
      {/if}
      <Button variant="primary-solid" size="sm" onclick={handleSave} disabled={!canSave || saving}>
        {saving ? $t("common.loading") : $t("common.save")}
      </Button>
    </DialogFooter>
  {/if}
</Dialog>
