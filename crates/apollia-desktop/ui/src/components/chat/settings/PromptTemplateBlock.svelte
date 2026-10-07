<script module lang="ts">
  import type { PromptTemplateSource } from "./usePromptTemplate.svelte";

  /** The i18n key of the line that says where the template comes from. */
  export const PROMPT_TEMPLATE_SOURCE_KEYS: Record<PromptTemplateSource, string> = {
    preset: "dev.chat.prompt_template.source_preset",
    engine: "dev.chat.prompt_template.source_engine",
    unavailable: "dev.chat.prompt_template.source_unavailable",
  };

  /**
   * Whether the Settings tab shows the block: the setting of Settings > Chat
   * is on, and so is the developer mode.
   */
  export function promptTemplateShown(setting: boolean, developerMode: boolean): boolean {
    return setting && developerMode;
  }
</script>

<script lang="ts">
  /**
   * The prompt template the model of the conversation receives, read only:
   * the one its preset carries, else the one the engine applies to the loaded
   * model. Shown under the preset, in developer mode, when Settings > Chat
   * asks for it.
   */
  import { untrack } from "svelte";
  import { t } from "svelte-i18n";
  import { Check, Copy } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { getChatPromptTemplate } from "$lib/ipc/chatSettings";
  import { loadedModels } from "$lib/stores/modelResidency";
  import type { ChatSessionDetail } from "$lib/types";
  import type { SessionSettingsController } from "./useSessionSettings.svelte";
  import {
    createPromptTemplate,
    promptTemplateKey,
    type PromptTemplateReader,
  } from "./usePromptTemplate.svelte";

  interface Props {
    session: ChatSessionDetail | null;
    controller: SessionSettingsController;
    /** The template shown; the block creates its own when left out. */
    reader?: PromptTemplateReader;
  }

  let { session, controller, reader = createPromptTemplate(getChatPromptTemplate) }: Props =
    $props();

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | null = null;

  const answer = $derived(reader.answer);
  // The settings of a conversation being read are not its own yet.
  const key = $derived(
    controller.loading
      ? null
      : promptTemplateKey({
          sessionId: session?.id ?? null,
          model: session?.llm_backend ?? null,
          preset: controller.storedPreset,
          loaded: $loadedModels,
          messages: session?.messages.length ?? 0,
        }),
  );

  // The block stays mounted across conversations: the template is read again
  // when the conversation, its model, its preset, the resident models or the
  // number of its messages change.
  let readFor: string | null = null;
  $effect(() => {
    const next = key;
    const id = session?.id ?? null;
    if (controller.loading || next === readFor) return;
    readFor = next;
    untrack(() => {
      if (next === null || id === null) reader.clear();
      else void reader.refresh(id);
    });
  });

  $effect(() => {
    return () => {
      if (copiedTimer) clearTimeout(copiedTimer);
    };
  });

  async function copy(): Promise<void> {
    if (answer.template === null) return;
    try {
      await navigator.clipboard.writeText(answer.template);
    } catch {
      // The clipboard may be refused; the text stays selectable.
      return;
    }
    copied = true;
    if (copiedTimer) clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied = false), 1600);
  }
</script>

<Disclosure open testid="session-prompt-template">
  {#snippet summary()}
    <span class="flex min-w-0 flex-col">
      <span class="text-xs font-medium text-foreground">
        {$t("dev.chat.prompt_template.title")}
      </span>
      <span
        class="truncate text-micro text-muted-foreground"
        data-testid="session-prompt-template-source"
        data-source={answer.source}
      >
        {$t(PROMPT_TEMPLATE_SOURCE_KEYS[answer.source])}
      </span>
    </span>
  {/snippet}
  <div class="space-y-2 px-3 pb-3">
    {#if answer.template !== null}
      <pre
        class="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-md border border-border bg-muted/40 p-2 font-mono text-micro text-foreground"
        data-testid="session-prompt-template-text">{answer.template}</pre>
      <div class="flex justify-end">
        <Button
          variant="ghost"
          size="sm"
          onclick={() => void copy()}
          data-testid="session-prompt-template-copy"
        >
          {#if copied}
            <Check size={12} aria-hidden="true" />
            {$t("dev.chat.prompt_template.copied")}
          {:else}
            <Copy size={12} aria-hidden="true" />
            {$t("dev.chat.prompt_template.copy")}
          {/if}
        </Button>
      </div>
    {:else}
      <p class="text-micro text-muted-foreground" data-testid="session-prompt-template-unavailable">
        {$t("dev.chat.prompt_template.unavailable")}
      </p>
    {/if}
  </div>
</Disclosure>
