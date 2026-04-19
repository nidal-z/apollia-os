<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import type { AgentTemplateType } from "$lib/types";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Dialog } from "$lib/components/ui/dialog";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Brain, MessageCircle, GitBranch, AlertTriangle, Loader2, Sparkles } from "lucide-svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
    oncreated: () => void;
  }

  let { open, onclose, oncreated }: Props = $props();

  const NAME_REGEX = /^[a-z][a-z0-9-]*[a-z0-9]$/;

  const templates: Array<{
    type: AgentTemplateType;
    titleKey: string;
    descKey: string;
    icon: typeof Brain;
    color: string;
    borderSelected: string;
    bgSelected: string;
  }> = [
    {
      type: "react",
      titleKey: "agents.template_react_title",
      descKey: "agents.template_react_desc",
      icon: Brain,
      color: "#3B82F6",
      borderSelected: "border-[#3B82F6]",
      bgSelected: "bg-[#3B82F6]/10",
    },
    {
      type: "conversational",
      titleKey: "agents.template_conversational_title",
      descKey: "agents.template_conversational_desc",
      icon: MessageCircle,
      color: "#8B5CF6",
      borderSelected: "border-[#8B5CF6]",
      bgSelected: "bg-[#8B5CF6]/10",
    },
    {
      type: "orchestrated",
      titleKey: "agents.template_orchestrated_title",
      descKey: "agents.template_orchestrated_desc",
      icon: GitBranch,
      color: "#3B82F6",
      borderSelected: "border-[#7C5FD6]",
      bgSelected: "bg-[#7C5FD6]/10",
    },
  ];

  let agentName = $state("");
  let selectedType = $state<AgentTemplateType | null>(null);
  let isCreating = $state(false);
  let errorMessage = $state("");
  let nameError = $state("");
  let sdkAvailable = $state<boolean | null>(null);

  let nameCheckTimeout: ReturnType<typeof setTimeout> | undefined;

  const canSubmit = $derived(
    sdkAvailable === true &&
    selectedType !== null &&
    agentName.length >= 2 &&
    !nameError &&
    !isCreating
  );

  async function checkSdk(): Promise<void> {
    sdkAvailable = null;
    try {
      sdkAvailable = await invoke<boolean>("check_sdk_available");
    } catch {
      sdkAvailable = false;
    }
  }

  function validateName(name: string): void {
    if (nameCheckTimeout !== undefined) {
      clearTimeout(nameCheckTimeout);
      nameCheckTimeout = undefined;
    }

    if (!name) {
      nameError = $t("agents.field_name_required");
      return;
    }
    if (name.length < 2) {
      nameError = $t("agents.field_name_too_short");
      return;
    }
    if (!NAME_REGEX.test(name)) {
      nameError = $t("agents.field_name_invalid");
      return;
    }

    nameError = "";
    nameCheckTimeout = setTimeout(async () => {
      try {
        const available = await invoke<boolean>("check_agent_name_available", { name });
        if (!available && agentName === name) {
          nameError = $t("agents.field_name_exists", { values: { name } });
        }
      } catch {
        // Best-effort check — ignore errors
      }
    }, 300);
  }

  function handleNameInput(event: Event): void {
    const input = event.target as HTMLInputElement;
    agentName = input.value;
    validateName(agentName);
  }

  function selectTemplate(type: AgentTemplateType): void {
    if (sdkAvailable !== true) return;
    selectedType = type;
  }

  async function handleCreate(): Promise<void> {
    if (!canSubmit || !selectedType) return;

    isCreating = true;
    errorMessage = "";
    try {
      await invoke("create_agent_from_template", {
        name: agentName,
        templateType: selectedType,
      });
      addToast($t("agents.create_success", { values: { name: agentName } }), "success");
      oncreated();
      handleClose();
    } catch (err: unknown) {
      errorMessage = err instanceof Error ? err.message : String(err);
    } finally {
      isCreating = false;
    }
  }

  function handleClose(): void {
    if (isCreating) return;
    agentName = "";
    selectedType = null;
    errorMessage = "";
    nameError = "";
    sdkAvailable = null;
    onclose();
  }

  function handleDialogClose(): void {
    if (isCreating) return;
    handleClose();
  }

  $effect(() => {
    if (open) {
      void checkSdk();
    }
  });
</script>

<Dialog
  open={open}
  onclose={handleDialogClose}
  size="lg"
  title={$t("agents.create_from_template")}
  data-testid="dialog-create-from-template"
>
  <div class="space-y-5">
    <!-- SDK warning banner -->
    {#if sdkAvailable === false}
      <div
        class="flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-3.5 py-2.5"
        data-testid="sdk-warning-banner"
      >
        <AlertTriangle size={16} class="mt-0.5 shrink-0 text-amber-500" />
        <div class="text-xs text-amber-200">
          <p>{$t("agents.sdk_not_available")}</p>
        </div>
      </div>
    {/if}

    <!-- SDK loading -->
    {#if sdkAvailable === null}
      <div class="flex items-center gap-2 text-xs text-muted-foreground">
        <Loader2 size={14} class="animate-spin" />
        {$t("agents.sdk_checking")}
      </div>
    {/if}

    <!-- Template selection -->
    <div>
      <p class="mb-2 block text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {$t("agents.select_template")}
      </p>
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
        {#each templates as tmpl}
          {@const isSelected = selectedType === tmpl.type}
          {@const isDisabled = sdkAvailable !== true}
          <button
            type="button"
            data-testid="template-card-{tmpl.type}"
            class="relative rounded-lg border p-3.5 text-left transition-all duration-200
              {isDisabled ? 'pointer-events-none opacity-50' : 'cursor-pointer hover:border-white/20'}
              {isSelected ? `${tmpl.borderSelected} ${tmpl.bgSelected} border-2` : 'border-white/10 bg-white/5 backdrop-blur-xl'}"
            onclick={() => selectTemplate(tmpl.type)}
            disabled={isDisabled}
          >
            <!-- SDK badge -->
            <span
              class="absolute right-2 top-2 rounded px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide"
              style="background: rgba(139, 92, 246, 0.2); color: #a78bfa;"
            >
              {$t("agents.template_badge_sdk")}
            </span>

            <!-- Icon -->
            <div
              class="mb-2.5 flex h-8 w-8 items-center justify-center rounded-md"
              style="background: {tmpl.color}20; color: {tmpl.color};"
            >
              <svelte:component this={tmpl.icon} size={16} />
            </div>

            <!-- Title -->
            <h3 class="text-sm font-medium text-foreground">{$t(tmpl.titleKey)}</h3>

            <!-- Description -->
            <p class="mt-1 text-[11px] leading-relaxed text-muted-foreground">{$t(tmpl.descKey)}</p>
          </button>
        {/each}
      </div>
    </div>

    <!-- Agent name input -->
    <div>
      <label class="mb-1 block text-[11px] text-muted-foreground" for="agent-name">
        {$t("agents.field_name")}
      </label>
      <Input
        id="agent-name"
        class={nameError ? "border-destructive" : ""}
        placeholder={$t("agents.field_name_placeholder")}
        value={agentName}
        oninput={handleNameInput}
        disabled={sdkAvailable !== true}
        data-testid="input-agent-name"
      />
      {#if nameError}
        <p class="mt-0.5 text-xs text-destructive" data-testid="name-error">{nameError}</p>
      {/if}
    </div>

    <!-- Error message from creation -->
    {#if errorMessage}
      <div
        class="rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive"
        data-testid="create-error"
      >
        {errorMessage}
      </div>
    {/if}

    <!-- Prevent close during creation -->
    {#if isCreating}
      <p class="text-xs text-muted-foreground">{$t("agents.creation_in_progress")}</p>
    {/if}

    <!-- Actions -->
    <div class="flex justify-end gap-2">
      <Button
        variant="outline"
        size="sm"
        onclick={handleClose}
        disabled={isCreating}
        data-testid="btn-cancel-create"
      >
        {$t("common.cancel")}
      </Button>
      <Button
        size="sm"
        onclick={handleCreate}
        disabled={!canSubmit}
        data-testid="btn-create-agent"
        class="gap-1.5"
      >
        {#if isCreating}
          <Loader2 size={13} class="animate-spin" />
          {$t("agents.creating")}
        {:else}
          <Sparkles size={13} />
          {$t("agents.create")}
        {/if}
      </Button>
    </div>
  </div>
</Dialog>
