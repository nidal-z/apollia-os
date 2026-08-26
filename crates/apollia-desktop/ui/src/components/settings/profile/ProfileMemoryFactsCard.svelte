<script lang="ts">
  /**
   * ProfileMemoryFactsCard - renders the off-schema memory facts (`extras`):
   * details agents retained over conversations. Previously loaded but never
   * shown; this is the new capability.
   *
   * Facts persist immediately (they are agent-owned memory, not part of the
   * form's explicit save): add / edit / delete each hit the profile store now,
   * with list-motion on insert and removal and a right-click context menu per
   * fact (see ProfileFactRow). Errors surface as humanized toasts.
   */
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { Brain, Plus } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import { reportError } from "$lib/errors/reportError";
  import { setProfileEntry, type ProfileEntryView } from "$lib/ipc/profile";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileFactRow from "./ProfileFactRow.svelte";

  interface Props {
    /** Initial off-schema facts loaded from `get_profile`. */
    facts: ProfileEntryView[];
  }

  let { facts }: Props = $props();

  // Seed the mutable list from the loaded facts; the card owns it afterwards
  // (a factsNonce remount reseeds it after a profile reset).
  // svelte-ignore state_referenced_locally
  let rows = $state<ProfileEntryView[]>([...facts]);
  let addOpen = $state(false);
  let addKey = $state("");
  let addValue = $state("");
  let adding = $state(false);

  function onUpdated(updated: ProfileEntryView): void {
    rows = rows.map((r) => (r.key === updated.key ? updated : r));
  }

  function onDeleted(key: string): void {
    rows = rows.filter((r) => r.key !== key);
  }

  async function addFact(): Promise<void> {
    const key = addKey.trim();
    const value = addValue.trim();
    if (!key || !value || adding) return;
    adding = true;
    try {
      const created = await setProfileEntry(key, value);
      rows = [created, ...rows.filter((r) => r.key !== key)];
      addKey = "";
      addValue = "";
      addOpen = false;
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      adding = false;
    }
  }
</script>

<SettingsSection
  title={$t("settings.profile.memory.title")}
  description={$t("settings.profile.memory.subtitle")}
  data-testid="profile-section-memory"
>
  {#snippet icon()}
    <Brain size={15} strokeWidth={1.75} />
  {/snippet}
  {#snippet actions()}
    <Button
      variant="ghost"
      size="sm"
      onclick={() => (addOpen = !addOpen)}
      data-testid="profile-fact-add-toggle"
    >
      {#snippet icon()}
        <Plus size={14} strokeWidth={2} />
      {/snippet}
      {$t("settings.profile.memory.add")}
    </Button>
  {/snippet}

  {#if addOpen}
    <div
      class="flex flex-col gap-2 rounded-lg border border-primary/30 bg-primary/5 p-3 sm:flex-row"
      transition:fly={rowIn()}
      data-testid="profile-fact-add-form"
    >
      <Input
        bind:value={addKey}
        placeholder={$t("settings.profile.memory.key_placeholder")}
        aria-label={$t("settings.profile.memory.key_placeholder")}
        class="sm:w-52"
        data-testid="profile-fact-add-key"
      />
      <Input
        bind:value={addValue}
        placeholder={$t("settings.profile.memory.value_placeholder")}
        aria-label={$t("settings.profile.memory.value_placeholder")}
        class="flex-1"
        onkeydown={(e: KeyboardEvent) => {
          if (e.key === "Enter") void addFact();
        }}
        data-testid="profile-fact-add-value"
      />
      <Button
        variant="default"
        size="sm"
        loading={adding}
        disabled={!addKey.trim() || !addValue.trim()}
        onclick={() => void addFact()}
        data-testid="profile-fact-add-confirm"
      >
        {$t("settings.profile.memory.add")}
      </Button>
    </div>
  {/if}

  {#if rows.length === 0}
    <div class="py-8 text-center" data-testid="profile-facts-empty">
      <span
        class="mx-auto mb-3 grid h-10 w-10 place-items-center rounded-full bg-primary/10 text-primary"
      >
        <Brain size={20} strokeWidth={1.75} aria-hidden="true" />
      </span>
      <p class="text-body-sm font-medium text-foreground">
        {$t("settings.profile.memory.empty.title")}
      </p>
      <p class="mx-auto mt-1 max-w-sm text-caption text-muted-foreground">
        {$t("settings.profile.memory.empty.detail")}
      </p>
    </div>
  {:else}
    <ul class="flex flex-col" data-testid="profile-facts-list">
      {#each rows as fact (fact.key)}
        <li animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
          <ProfileFactRow {fact} {onUpdated} {onDeleted} />
        </li>
      {/each}
    </ul>
  {/if}
</SettingsSection>
