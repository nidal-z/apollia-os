<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.profile",
    icon: "user",
    group: "settings.nav.cluster_personalization",
    cluster: "personalization",
  } as const;
</script>

<script lang="ts">
  /**
   * Settings > Profile - the user's identity, agent-supervision preferences,
   * tools, constraints, and the off-schema memory facts.
   *
   * The profile is a flat key/value store (`__user__`) whose schema is declared
   * in Rust (PROFILE_SCHEMA); this form mirrors it by hand. It is an
   * explicit-save page: field edits accumulate locally and persist together via
   * the shared frame's Save (footer / Cmd+S / nav guard). Memory facts are
   * agent-owned and persist immediately, outside the form's save cycle.
   */
  import { onMount } from "svelte";
  import { setRouteState } from "$lib/stores/settingsDirty";
  import { reportError } from "$lib/errors/reportError";
  import {
    getProfile,
    setProfileEntry,
    deleteProfileEntry,
    type ProfileEntryView,
  } from "$lib/ipc/profile";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import ProfileEnrichmentBanner from "../../components/settings/profile/ProfileEnrichmentBanner.svelte";
  import ProfileIdentityCard from "../../components/settings/profile/ProfileIdentityCard.svelte";
  import ProfileSupervisionCard from "../../components/settings/profile/ProfileSupervisionCard.svelte";
  import ProfileToolsCard from "../../components/settings/profile/ProfileToolsCard.svelte";
  import ProfileConstraintsCard from "../../components/settings/profile/ProfileConstraintsCard.svelte";
  import ProfilePreferencesCard from "../../components/settings/profile/ProfilePreferencesCard.svelte";
  import ProfileMemoryFactsCard from "../../components/settings/profile/ProfileMemoryFactsCard.svelte";
  import ProfileDangerZone from "../../components/settings/profile/ProfileDangerZone.svelte";
  import type { ProfileFieldApi } from "../../components/settings/profile/profileForm";

  let loading = $state(true);
  let values = $state<Record<string, string>>({});
  let sources = $state<Record<string, string>>({});
  let original = $state<Record<string, string>>({});
  let facts = $state<ProfileEntryView[]>([]);
  let factsNonce = $state(0);

  const api: ProfileFieldApi = {
    get values() {
      return values;
    },
    get sources() {
      return sources;
    },
    set: (key: string, value: string) => {
      values[key] = value;
    },
  };

  onMount(async () => {
    try {
      const profile = await getProfile();
      const valMap: Record<string, string> = {};
      const srcMap: Record<string, string> = {};
      for (const e of profile.entries ?? []) {
        valMap[e.key] = e.value;
        srcMap[e.key] = e.written_by;
      }
      values = valMap;
      sources = srcMap;
      original = { ...valMap };
      facts = profile.extras ?? [];
    } catch {
      // A missing profile is an empty form, not an error.
      values = {};
      sources = {};
      original = {};
      facts = [];
    } finally {
      loading = false;
    }
  });

  function changedKeys(): string[] {
    const keys = new Set([...Object.keys(original), ...Object.keys(values)]);
    const out: string[] = [];
    for (const key of keys) {
      const cur = (values[key] ?? "").trim();
      const orig = (original[key] ?? "").trim();
      if (cur !== orig) out.push(key);
    }
    return out;
  }

  // Keep the shell's dirty affordance (nav guard, sidebar dot, Save button) in
  // sync with local edits.
  $effect(() => {
    if (loading) return;
    setRouteState("profile", { dirty: changedKeys().length > 0 });
  });

  async function save(): Promise<boolean> {
    setRouteState("profile", { savingState: "saving", lastError: null });
    const changed = changedKeys();
    try {
      for (const key of changed) {
        const next = (values[key] ?? "").trim();
        if (next === "") {
          try {
            await deleteProfileEntry(key);
          } catch {
            // Ignore NOT_FOUND: an emptied field that was never persisted.
          }
        } else {
          await setProfileEntry(key, next);
        }
      }
      for (const key of changed) {
        const next = (values[key] ?? "").trim();
        if (next === "") {
          delete values[key];
          delete sources[key];
        } else {
          values[key] = next;
          sources[key] = "user";
        }
      }
      original = { ...values };
      setRouteState("profile", { dirty: false, savingState: "saved" });
      return true;
    } catch (err) {
      const humanized = reportError(err, { surface: "inline" });
      setRouteState("profile", {
        savingState: "error",
        lastError: humanized.friendly_message,
      });
      return false;
    }
  }

  function reset(): void {
    values = { ...original };
    setRouteState("profile", { dirty: false, savingState: "idle" });
  }

  function afterProfileReset(): void {
    values = {};
    sources = {};
    original = {};
    facts = [];
    factsNonce += 1;
    setRouteState("profile", { dirty: false, savingState: "idle" });
  }
</script>

{#if loading}
  <SettingSectionSkeleton />
{:else}
  <SettingsSubPage
    route="profile"
    onSave={save}
    onReset={reset}
    data-testid="profile-section"
  >
    <ProfileEnrichmentBanner {api} />
    <ProfileIdentityCard {api} />
    <ProfileSupervisionCard {api} />
    <ProfileToolsCard {api} />
    <ProfileConstraintsCard {api} />
    {#key factsNonce}
      <ProfileMemoryFactsCard {facts} />
    {/key}
    <ProfilePreferencesCard {api} />
    <ProfileDangerZone onReset={afterProfileReset} />
  </SettingsSubPage>
{/if}
