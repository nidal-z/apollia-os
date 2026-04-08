<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import type { ProjectDetail } from "$lib/types";

  interface Props {
    open: boolean;
    onclose: () => void;
    oncreated: (id: string, name: string) => void;
  }

  let { open, onclose, oncreated }: Props = $props();

  let name = $state("");
  let description = $state("");
  let instructions = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);

  function reset() {
    name = "";
    description = "";
    instructions = "";
    error = null;
    submitting = false;
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
    submitting = true;
    error = null;
    try {
      const project = await invoke<ProjectDetail>("create_project", {
        request: {
          name: name.trim(),
          description: description.trim() || undefined,
          instructions: instructions.trim() || undefined,
        },
      });
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
    <div class="space-y-1.5">
      <label class="text-sm font-medium" for="project-name">{$t("projects.field_name")}</label>
      <Input
        id="project-name"
        bind:value={name}
        placeholder={$t("projects.field_name_placeholder")}
        disabled={submitting}
        data-testid="project-name-input"
      />
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
      <Button onclick={handleSubmit} disabled={submitting || !name.trim()} data-testid="create-project-submit">
        {submitting ? $t("common.submitting") : $t("projects.create_project")}
      </Button>
    </div>
  </div>
</Dialog>
