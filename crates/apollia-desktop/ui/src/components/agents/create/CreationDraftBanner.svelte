<script lang="ts">
  /**
   * CreationDraftBanner - the creation of an agent left under way, above the
   * list of agents: where it stands, a way back to it, and a way to discard
   * it. The workshop it may be running goes on while the banner is shown.
   */
  import { t } from "svelte-i18n";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { cancelWorkflowRun } from "$lib/ipc/workflows";
  import { PHASE_LABEL, creationDraft, draftStarted, endCreation } from "./creationDraft";

  interface Props {
    onresume: () => void;
  }

  let { onresume }: Props = $props();

  const draft = creationDraft("agents");
  let discarding = $state(false);

  const shown = $derived(draftStarted($draft) ? $draft : null);
  const title = $derived(shown?.review?.name.trim() || shown?.wish.purpose.trim() || "");

  function discard(): void {
    discarding = false;
    const runId = $draft?.runId ?? null;
    if (runId) void cancelWorkflowRun(runId).catch(() => undefined);
    endCreation("agents");
  }
</script>

{#if shown}
  <Banner variant="info" surface="card" data-testid="agent-creation-draft">
    <p class="m-0 font-medium">{$t("agents.creation_draft.title")}</p>
    <p class="m-0 text-caption opacity-90">
      {$t(PHASE_LABEL[shown.phase])}{#if title !== ""}{" "}<span class="break-words">« {title.length > 80 ? `${title.slice(0, 80)}…` : title} »</span>{/if}
    </p>
    {#snippet trailing()}
      <div class="flex gap-2">
        <Button variant="ghost" size="sm" onclick={() => (discarding = true)} data-testid="agent-creation-draft-discard">
          {$t("agent_create.discard")}
        </Button>
        <Button variant="primary-solid" size="sm" onclick={onresume} data-testid="agent-creation-draft-resume">
          {$t("agents.creation_draft.resume")}
        </Button>
      </div>
    {/snippet}
  </Banner>
{/if}

<ConfirmDialog
  open={discarding}
  onclose={() => (discarding = false)}
  onconfirm={discard}
  title={$t("agent_create.discard_title")}
  message={$t("agent_create.discard_message")}
  confirmLabel={$t("agent_create.discard_confirm")}
  data-testid="agent-creation-draft-discard-dialog"
/>
