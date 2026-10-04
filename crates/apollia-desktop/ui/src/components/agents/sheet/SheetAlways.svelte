<script lang="ts">
  /**
   * SheetAlways - the standing consents of the agent: each family the user
   * answered "always" for, withdrawn in one click exactly as on the
   * authorizations screen, with the same undo in the line.
   */
  import { t } from "svelte-i18n";
  import { reportError } from "$lib/errors/reportError";
  import { listFamilyConsents, type FamilyConsent } from "$lib/ipc/permissions";
  import { createIdentityGuard } from "$lib/utils/identityGuard";
  import AlwaysRow from "../../authorizations/AlwaysRow.svelte";
  import { shellIpc, undoAlways, withdrawAlways } from "../../authorizations/authorizationActions";
  import { alwaysOf, consentKey } from "../../authorizations/authorizations";
  import SheetSection from "./SheetSection.svelte";

  interface Props {
    /** Identifier of the agent's definition: its name, or `apollia:chat`. */
    definitionId: string;
  }

  let { definitionId }: Props = $props();

  let consents = $state<FamilyConsent[]>([]);
  let withdrawn = $state<string[]>([]);
  let busy = $state<string | null>(null);
  let loading = $state(true);

  const guard = createIdentityGuard(() => definitionId);

  $effect(() => {
    void load(definitionId);
  });

  async function load(id: string): Promise<void> {
    const ticket = guard.begin();
    loading = true;
    const listed = await listFamilyConsents().catch(() => [] as FamilyConsent[]);
    if (!ticket.current) return;
    consents = alwaysOf(listed, id);
    withdrawn = [];
    loading = false;
  }

  async function change(consent: FamilyConsent, undo: boolean): Promise<void> {
    const key = consentKey(consent);
    busy = key;
    try {
      if (undo) {
        await undoAlways(shellIpc, consent);
        withdrawn = withdrawn.filter((k) => k !== key);
      } else {
        await withdrawAlways(shellIpc, consent);
        withdrawn = [...withdrawn, key];
      }
    } catch (err) {
      reportError(err, { surface: "toast", testid: "agent-sheet-always-error-toast" });
    } finally {
      busy = null;
    }
  }
</script>

<SheetSection
  title={$t("authorizations.always_title")}
  legend={$t("agent_sheet.always_legend")}
  testid="agent-sheet-always"
>
  {#if loading}
    <p class="m-0 text-body-xs text-muted-foreground">{$t("common.loading")}</p>
  {:else if consents.length === 0}
    <p class="m-0 text-caption italic text-muted-foreground" data-testid="agent-sheet-always-empty">
      {$t("authorizations.always_empty")}
    </p>
  {:else}
    <ul class="m-0 list-none p-0">
      {#each consents as consent (consentKey(consent))}
        <AlwaysRow
          {consent}
          withdrawn={withdrawn.includes(consentKey(consent))}
          busy={busy === consentKey(consent)}
          onWithdraw={(c) => void change(c, false)}
          onUndo={(c) => void change(c, true)}
        />
      {/each}
    </ul>
  {/if}
</SheetSection>
