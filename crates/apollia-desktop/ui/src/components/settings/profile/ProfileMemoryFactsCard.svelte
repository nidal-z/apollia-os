<script lang="ts">
  /**
   * ProfileMemoryFactsCard - the facts retained in the displayed profile:
   * written by its agents or added by the user, used by this profile only.
   *
   * Facts persist immediately, outside the form's explicit save: add, edit and
   * delete each reach the profile store now, with list motion on insert and
   * removal and a right-click context menu per fact (see ProfileFactRow). A
   * fact is added as one sentence; its key is typed only in developer mode,
   * derived from the sentence otherwise (`addFact`).
   */
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { Brain, Plus } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import { reportError } from "$lib/errors/reportError";
  import { setProfileFact, type ProfileEntryView } from "$lib/ipc/profile";
  import { developerMode } from "$lib/stores/developerMode";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileFactRow from "./ProfileFactRow.svelte";
  import { addFact as addFactTo } from "./profileForm";

  interface Props {
    /** Identifier of the displayed profile. */
    profileId: string;
    /** Name of the displayed profile, for the title. */
    profileName: string;
    /** Facts of the profile as loaded with it. */
    facts: ProfileEntryView[];
  }

  let { profileId, profileName, facts }: Props = $props();

  // Seed the mutable list from the loaded facts; the card owns it afterwards
  // (the page remounts it on a change of profile or after a reset).
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

  function closeAdd(): void {
    addKey = "";
    addValue = "";
    addOpen = false;
  }

  async function addFact(): Promise<void> {
    if (adding) return;
    adding = true;
    try {
      const created = await addFactTo(
        profileId,
        addValue,
        $developerMode ? addKey : null,
        rows,
        { set: setProfileFact },
      );
      if (created) {
        rows = [created, ...rows.filter((r) => r.key !== created.key)];
        closeAdd();
      }
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      adding = false;
    }
  }
</script>

<SettingsSection
  title={$t("settings.profile.memory.title", { values: { name: profileName } })}
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
      class="flex flex-col gap-2 rounded-lg border border-primary/30 bg-primary/5 p-3"
      transition:fly={rowIn()}
      data-testid="profile-fact-add-form"
    >
      <div class="flex flex-col gap-2 sm:flex-row">
        {#if $developerMode}
          <Input
            bind:value={addKey}
            placeholder={$t("settings.profile.memory.key_placeholder")}
            aria-label={$t("settings.profile.memory.key_placeholder")}
            class="sm:w-52"
            data-testid="profile-fact-add-key"
          />
        {/if}
        <Input
          bind:value={addValue}
          placeholder={$t("settings.profile.memory.value_placeholder")}
          aria-label={$t("settings.profile.memory.value_placeholder")}
          class="flex-1"
          onkeydown={(e: KeyboardEvent) => {
            if (e.key === "Enter") void addFact();
            else if (e.key === "Escape") closeAdd();
          }}
          data-testid="profile-fact-add-value"
        />
        <Button variant="ghost" size="sm" onclick={closeAdd} data-testid="profile-fact-add-cancel">
          {$t("common.cancel")}
        </Button>
        <Button
          variant="default"
          size="sm"
          loading={adding}
          disabled={!addValue.trim() || ($developerMode && !addKey.trim())}
          onclick={() => void addFact()}
          data-testid="profile-fact-add-confirm"
        >
          {$t("settings.profile.memory.add_confirm")}
        </Button>
      </div>
      <p class="text-caption text-muted-foreground">{$t("settings.profile.memory.add_hint")}</p>
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
          <ProfileFactRow {profileId} {fact} {onUpdated} {onDeleted} />
        </li>
      {/each}
    </ul>
  {/if}
</SettingsSection>
