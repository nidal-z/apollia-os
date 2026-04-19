<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
  import { t } from "svelte-i18n";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { FolderOpen } from "lucide-svelte";
  import type { ProjectDetail, ProjectTemplate } from "$lib/types";

  interface Props {
    open: boolean;
    onclose: () => void;
    oncreated: (id: string, name: string) => void;
  }

  let { open, onclose, oncreated }: Props = $props();

  let name = $state("");
  let description = $state("");
  let instructions = $state("");
  let workspacePath = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let templates = $state<ProjectTemplate[]>([]);
  let selectedTemplateId = $state<string | null>(null);

  $effect(() => {
    if (open) { void loadTemplates(); }
  });

  async function loadTemplates(): Promise<void> {
    try {
      templates = await invoke<ProjectTemplate[]>("list_project_templates");
    } catch {
      templates = [];
    }
  }

  function reset() {
    name = "";
    description = "";
    instructions = "";
    workspacePath = "";
    selectedTemplateId = null;
    error = null;
    submitting = false;
  }

  async function onNameBlur(): Promise<void> {
    if (!name.trim()) return;
    try {
      workspacePath = await invoke<string>("suggest_workspace_path", {
        projectName: name.trim(),
      });
    } catch {
      // Suggestion non critique — on laisse le champ vide
    }
  }

  function parentDir(p: string): string {
    const lastSlash = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return lastSlash > 0 ? p.slice(0, lastSlash) : p;
  }

  async function pickWorkspaceFolder(): Promise<void> {
    const defaultPath = workspacePath ? parentDir(workspacePath) : undefined;
    const selected = await openFolderPicker({
      directory: true,
      multiple: false,
      defaultPath,
      title: $t("projects.pick_workspace"),
    });
    if (typeof selected === "string" && selected) {
      workspacePath = selected;
    }
  }

  function handleClose() {
    reset();
    onclose();
  }

  async function handleSubmit(): Promise<void> {
    if (!name.trim()) {
      error = $t("projects.name_required");
      return;
    }
    if (!workspacePath) {
      error = $t("projects.workspace_required");
      return;
    }
    submitting = true;
    error = null;
    try {
      const project = await invoke<ProjectDetail>("create_project", {
        request: {
          name: name.trim(),
          description: description.trim() || undefined,
          instructions: instructions.trim() || undefined,
          workspace_path: workspacePath,
        },
      });

      // Apply template providers if a template was selected
      if (selectedTemplateId) {
        const template = templates.find((t) => t.id === selectedTemplateId);
        if (template) {
          try {
            const providerConfigs = JSON.parse(template.providers_config_json) as Array<{
              provider_type: string;
              name: string;
              enabled: boolean;
              priority: number;
            }>;
            for (const cfg of providerConfigs) {
              await invoke("set_project_provider", {
                projectId: project.id,
                providerType: cfg.provider_type,
                name: cfg.name,
                configJson: "{}",
                path: null,
                enabled: cfg.enabled,
                priority: cfg.priority,
              });
            }
          } catch {
            // Template application failed — project still created, providers can be added manually.
          }
        }
      }

      reset();
      oncreated(project.id, project.name);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }
</script>

<Dialog {open} onclose={handleClose} title={$t("projects.new_project")} size="md">
  <div class="space-y-4">
    <!-- Template selector -->
    {#if templates.length > 0}
      <div class="space-y-1.5">
        <p class="text-sm font-medium" id="template-group-caption">{$t("projects.choose_template")}</p>
        <div class="flex gap-2" role="radiogroup" aria-labelledby="template-group-caption">
          {#each templates as template (template.id)}
            <button
              type="button"
              class="flex-1 rounded-lg border p-3 text-left text-sm transition-colors
                {selectedTemplateId === template.id
                  ? 'border-primary bg-primary/5 ring-1 ring-primary/20'
                  : 'border-border hover:bg-muted/40'}"
              onclick={() => { selectedTemplateId = selectedTemplateId === template.id ? null : template.id; }}
              disabled={submitting}
            >
              <p class="font-medium">{template.name}</p>
              {#if template.description}
                <p class="text-xs text-muted-foreground mt-0.5">{template.description}</p>
              {/if}
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="space-y-1.5">
      <label class="text-sm font-medium" for="project-name">{$t("projects.field_name")}</label>
      <Input
        id="project-name"
        bind:value={name}
        placeholder={$t("projects.field_name_placeholder")}
        disabled={submitting}
        onblur={onNameBlur}
        data-testid="project-name-input"
      />
    </div>

    <div class="space-y-1.5">
      <label class="text-sm font-medium" for="project-workspace">
        {$t("projects.field_workspace")}
      </label>
      <div class="flex gap-2">
        <input
          id="project-workspace"
          readonly
          value={workspacePath}
          placeholder={$t("projects.field_workspace_placeholder")}
          disabled={submitting}
          class="flex h-9 w-full flex-1 rounded-md border border-input bg-muted/30 px-3 py-1 text-sm shadow-sm cursor-default select-none"
          data-testid="project-workspace-display"
        />
        <Button
          variant="outline"
          size="sm"
          onclick={pickWorkspaceFolder}
          disabled={submitting}
        >
          <FolderOpen size={14} class="mr-1" />
          {$t("projects.browse")}
        </Button>
      </div>
      <p class="text-xs text-muted-foreground">{$t("projects.workspace_hint")}</p>
    </div>

    <div class="space-y-1.5">
      <label class="text-sm font-medium" for="project-description">
        {$t("projects.field_description")}
        <span class="text-muted-foreground font-normal text-xs ml-1">({$t("common.optional")})</span>
      </label>
      <Input
        id="project-description"
        bind:value={description}
        placeholder={$t("projects.field_description_placeholder")}
        disabled={submitting}
      />
    </div>

    <div class="space-y-1.5">
      <label class="text-sm font-medium" for="project-instructions">
        {$t("projects.field_instructions")}
        <span class="text-muted-foreground font-normal text-xs ml-1">({$t("common.optional")})</span>
      </label>
      <Textarea
        id="project-instructions"
        bind:value={instructions}
        placeholder={$t("projects.field_instructions_placeholder")}
        rows={4}
        disabled={submitting}
        class="resize-none"
      />
      <p class="text-xs text-muted-foreground">{$t("projects.instructions_hint")}</p>
    </div>

    {#if error}
      <p class="text-sm text-destructive">{error}</p>
    {/if}

    <div class="flex justify-end gap-2 pt-2">
      <Button variant="outline" onclick={handleClose} disabled={submitting}>
        {$t("common.cancel")}
      </Button>
      <Button
        onclick={handleSubmit}
        disabled={submitting || !name.trim() || !workspacePath}
        data-testid="create-project-submit"
      >
        {submitting ? $t("common.submitting") : $t("projects.create_project")}
      </Button>
    </div>
  </div>
</Dialog>
