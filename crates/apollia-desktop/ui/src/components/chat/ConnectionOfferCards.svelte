<script lang="ts">
  /**
   * The offers under a step where Apollia looked for a connection it lacks.
   *
   * A service that is not connected offers to connect it: the one flow of an
   * addition opens on its entry, in a dialog above the conversation. Once it
   * is added, its tools are proposed to this conversation and the request is
   * played again. A service connected but not granted to Apollia offers to
   * grant it, with no addition, then plays the request again.
   *
   * Playing the request again regenerates the last answer of the
   * conversation, so the offers stand only under that answer: under an older
   * one, another turn has followed, and an offer shows nothing until it is
   * taken. Once taken, the card says the request is played again only when it
   * was.
   */
  import { onMount } from "svelte";
  import { locale, t } from "svelte-i18n";
  import { CircleCheck, Plug } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Dialog } from "$lib/components/ui/dialog";
  import AddFlow from "../connections/add/AddFlow.svelte";
  import { APOLLIA_AGENT_ID } from "$lib/chat/chatTargets";
  import { connectionOffers, proposeConnection, type ConnectionOffer } from "$lib/chat/connectionOffer";
  import { catalogueKey } from "$lib/connections/addWindow";
  import { grantToAgents } from "$lib/connections/addFlow";
  import { entryLabel } from "$lib/connections/catalogueText";
  import { fetchMcpCurated } from "$lib/ipc/connections";
  import { findSelectionEntry } from "$lib/stores/connectionAdd";
  import { regenerableAnswers, requestRegenerateLast } from "$lib/stores/chatRegenerate";
  import type { RegistryServerView } from "$lib/types";

  interface Props {
    /** The output of the step. */
    output: string | null;
    /** The conversation the request is played again in. */
    sessionId: string;
    /** The answer the step belongs to: the offers stand only under the last one. */
    messageId?: string;
  }

  const { output, sessionId, messageId }: Props = $props();

  // `done` once the request is played again, `settled` when nothing was.
  type OfferState = "idle" | "working" | "done" | "settled" | "failed";

  let selection = $state<RegistryServerView[]>([]);
  let states = $state<Record<string, OfferState>>({});
  const lastTurn = $derived(messageId !== undefined && $regenerableAnswers[sessionId] === messageId);
  const offers = $derived(
    connectionOffers(output).filter((offer) => {
      if (lastTurn) return true;
      const state = states[offer.entry] ?? "idle";
      return state === "done" || state === "settled";
    }),
  );
  let adding = $state<{ offer: ConnectionOffer; entry: RegistryServerView } | null>(null);
  // The connection the open flow added, so that closing the dialog once it
  // exists still plays the request again.
  let addedId: string | null = null;

  onMount(() => {
    void fetchMcpCurated()
      .then((entries) => (selection = entries))
      .catch(() => (selection = []));
  });

  function nameOf(offer: ConnectionOffer): string {
    const entry = findSelectionEntry(selection, offer.entry);
    return entry ? entryLabel(entry, $locale) : offer.label;
  }

  function set(offer: ConnectionOffer, state: OfferState): void {
    states = { ...states, [offer.entry]: state };
  }

  async function openAdd(offer: ConnectionOffer): Promise<void> {
    let entries = selection;
    if (entries.length === 0) {
      try {
        entries = await fetchMcpCurated();
        selection = entries;
      } catch {
        entries = [];
      }
    }
    const entry = findSelectionEntry(entries, offer.entry);
    if (!entry) {
      set(offer, "failed");
      return;
    }
    set(offer, "idle");
    addedId = null;
    adding = { offer, entry };
  }

  /** The addition is over: its tools go to this conversation, then the request is played again. */
  async function added(offer: ConnectionOffer, connectionId: string | null): Promise<void> {
    adding = null;
    addedId = null;
    if (connectionId === null) return;
    if ((states[offer.entry] ?? "idle") !== "idle") return;
    set(offer, "working");
    try {
      await proposeConnection(sessionId, connectionId);
    } catch {
      // The tools could not be proposed: the connection exists, and the
      // request is played again with what the agent holds.
    }
    playAgain(offer);
  }

  /** Plays the request again from this answer, if it is still the last one. */
  function playAgain(offer: ConnectionOffer): void {
    const started = messageId !== undefined && requestRegenerateLast(sessionId, messageId);
    set(offer, started ? "done" : "settled");
  }

  async function grant(offer: ConnectionOffer & { kind: "grant" }): Promise<void> {
    set(offer, "working");
    try {
      const granted = await grantToAgents(offer.connection, [APOLLIA_AGENT_ID]);
      if (granted === 0) {
        set(offer, "failed");
        return;
      }
    } catch {
      set(offer, "failed");
      return;
    }
    playAgain(offer);
  }
</script>

{#if offers.length > 0}
  <ul class="mt-1 flex flex-col gap-2 pl-6" data-testid="connection-offers">
    {#each offers as offer (offer.entry)}
      {@const name = nameOf(offer)}
      {@const state = states[offer.entry] ?? "idle"}
      <li
        class="flex items-center gap-3 rounded-lg border border-border bg-card px-3 py-2.5"
        data-testid="connection-offer"
        data-kind={offer.kind}
        data-state={state}
      >
        <span
          class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-muted text-primary"
          aria-hidden="true"
        >
          <Plug size={14} strokeWidth={2} />
        </span>
        <span class="flex min-w-0 flex-1 flex-col gap-0.5">
          <span class="text-body-sm font-medium text-foreground">{name}</span>
          {#if state === "failed"}
            <span class="text-caption text-destructive" role="alert" data-testid="connection-offer-error">
              {$t(offer.kind === "add" ? "chat.connection_offer.add_failed" : "chat.connection_offer.grant_failed")}
            </span>
          {:else}
            <span class="text-caption text-muted-foreground">
              {$t(offer.kind === "add" ? "chat.connection_offer.add_body" : "chat.connection_offer.grant_body", {
                values: { name },
              })}
            </span>
          {/if}
        </span>
        {#if state === "done" || state === "settled"}
          <span
            class="flex shrink-0 items-center gap-1.5 text-caption text-success"
            role="status"
            data-testid="connection-offer-done"
          >
            <CircleCheck size={14} strokeWidth={2} aria-hidden="true" />
            {$t(state === "done" ? "chat.connection_offer.done" : "chat.connection_offer.settled")}
          </span>
        {:else if offer.kind === "add"}
          <Button
            variant="primary-solid"
            size="sm"
            disabled={state === "working"}
            onclick={() => void openAdd(offer)}
            data-testid="connection-offer-add"
          >
            {$t("chat.connection_offer.add", { values: { name } })}
          </Button>
        {:else}
          {@const toGrant = offer}
          <Button
            variant="primary-solid"
            size="sm"
            disabled={state === "working"}
            onclick={() => void grant(toGrant)}
            data-testid="connection-offer-grant"
          >
            {$t("chat.connection_offer.grant")}
          </Button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

{#if adding !== null}
  {@const current = adding}
  {@const name = entryLabel(current.entry, $locale)}
  <Dialog open={true} onclose={() => void added(current.offer, addedId)} size="lg" bodyClass="p-0" label={name}>
    <AddFlow
      entry={{ key: catalogueKey(current.entry), kind: "catalogue", entry: current.entry, source: "selection" }}
      {name}
      onback={() => (adding = null)}
      onadded={(connectionId) => (addedId = connectionId)}
      ondone={(connectionId) => void added(current.offer, connectionId)}
      onopenrule={() => (adding = null)}
    />
  </Dialog>
{/if}
