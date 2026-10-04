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
   * Settings > Profile - the profiles of the user: the list with the default
   * marked, the fields of the displayed profile, the facts retained in it.
   *
   * The fields come from Rust with the profile (`get_profile_by_id`); this page
   * carries no copy of the schema. Field edits accumulate locally and persist
   * together via the shared frame's Save (footer / Cmd+S / nav guard), and are
   * saved before another profile is displayed. Profile changes (create,
   * rename, default, reset, delete) and facts persist immediately.
   *
   * A proposal of the onboarding agent arrives here as unsaved edits of that
   * same form: the banner says so, "Save" is the Save of the frame and "Skip"
   * its Reset. The agent itself writes nothing.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { setRouteState } from "$lib/stores/settingsDirty";
  import { pendingProfileProposal } from "$lib/stores/onboarding";
  import { layProposal } from "$lib/onboarding/profileProposal";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import {
    createProfile,
    deleteProfile,
    getProfileById,
    listProfiles,
    resetProfile,
    setDefaultProfile,
    triggerOnboarding,
    updateProfile,
    type ProfileDetailView,
    type ProfileSummaryView,
  } from "$lib/ipc/profile";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import ProfileEnrichmentBanner from "../../components/settings/profile/ProfileEnrichmentBanner.svelte";
  import ProfileListCard from "../../components/settings/profile/ProfileListCard.svelte";
  import ProfileFieldsCard from "../../components/settings/profile/ProfileFieldsCard.svelte";
  import ProfileMemoryFactsCard from "../../components/settings/profile/ProfileMemoryFactsCard.svelte";
  import {
    applyReset,
    changedFields,
    fieldValues,
    profileAfterDelete,
    type ResetChoice,
  } from "../../components/settings/profile/profileForm";

  let loading = $state(true);
  let profiles = $state<ProfileSummaryView[]>([]);
  let selectedId = $state<string | null>(null);
  let detail = $state<ProfileDetailView | null>(null);
  let values = $state<Record<string, string>>({});
  let factsNonce = $state(0);
  // A proposal of the agent is laid over the form, not saved yet.
  let reviewing = $state(false);

  const changed = $derived(detail ? changedFields(detail.fields, values) : {});
  const dirty = $derived(Object.keys(changed).length > 0);

  function show(next: ProfileDetailView): void {
    detail = next;
    selectedId = next.profile.id;
    values = fieldValues(next.fields);
    factsNonce += 1;
    reviewing = false;
  }

  // The proposal a conversation hands back: laid over the form of the profile
  // it was asked for, as edits the person reads over. A proposal for another
  // profile, or one that changes nothing, is dropped.
  $effect(() => {
    const pending = $pendingProfileProposal;
    if (!pending || !detail) return;
    pendingProfileProposal.set(null);
    const next = layProposal(detail.profile.id, detail.fields, values, pending);
    if (!next) return;
    values = next;
    reviewing = true;
  });

  async function refreshList(): Promise<void> {
    profiles = await listProfiles();
  }

  async function open(id: string): Promise<void> {
    show(await getProfileById(id));
  }

  onMount(async () => {
    try {
      await refreshList();
      const first = profiles.find((p) => p.is_default) ?? profiles[0];
      if (first) await open(first.id);
    } catch (err) {
      reportError(err, { surface: "inline" });
    } finally {
      loading = false;
    }
  });

  // Keep the shell's dirty affordance (nav guard, sidebar dot, Save button) in
  // sync with local edits.
  $effect(() => {
    if (loading) return;
    setRouteState("profile", { dirty });
  });

  async function save(): Promise<boolean> {
    if (!detail) return true;
    setRouteState("profile", { savingState: "saving", lastError: null });
    try {
      show(await updateProfile(detail.profile.id, { fields: changed }));
      await refreshList();
      setRouteState("profile", { dirty: false, savingState: "saved" });
      reviewing = false;
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
    if (detail) values = fieldValues(detail.fields);
    reviewing = false;
    setRouteState("profile", { dirty: false, savingState: "idle" });
  }

  async function select(id: string): Promise<void> {
    if (id === selectedId) return;
    if (dirty && !(await save())) return;
    try {
      await open(id);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  async function attempt(action: () => Promise<void>): Promise<boolean> {
    try {
      await action();
      return true;
    } catch (err) {
      reportError(err, { surface: "toast" });
      return false;
    }
  }

  function onCreate(name: string): Promise<boolean> {
    return attempt(async () => {
      const created = await createProfile(name);
      await refreshList();
      if (dirty) await save();
      await open(created.id);
      addToast($t("settings.profile.toast.created", { values: { name: created.name } }), "success");
    });
  }

  function onRename(profile: ProfileSummaryView, name: string): Promise<boolean> {
    return attempt(async () => {
      const renamed = await updateProfile(profile.id, { name });
      await refreshList();
      if (renamed.profile.id === selectedId && detail) {
        detail = { ...detail, profile: renamed.profile };
      }
    });
  }

  async function onSetDefault(profile: ProfileSummaryView): Promise<void> {
    await attempt(async () => {
      await setDefaultProfile(profile.id);
      await refreshList();
      addToast(
        $t("settings.profile.toast.default_set", { values: { name: profile.name } }),
        "success",
      );
    });
  }

  function onReset(profile: ProfileSummaryView, choice: ResetChoice): Promise<boolean> {
    return attempt(async () => {
      await applyReset(profile, choice, { reset: resetProfile, onboard: triggerOnboarding });
      await refreshList();
      if (profile.id === selectedId) await open(profile.id);
      addToast($t("settings.profile.toast.reset_done"), "success");
    });
  }

  function onDelete(profile: ProfileSummaryView): Promise<boolean> {
    return attempt(async () => {
      await deleteProfile(profile.id);
      const next = profileAfterDelete(profiles, profile.id);
      await refreshList();
      if (profile.id === selectedId && next) await open(next);
      addToast($t("settings.profile.toast.deleted"), "success");
    });
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
    <ProfileListCard
      {profiles}
      {selectedId}
      onSelect={(id: string) => void select(id)}
      {onCreate}
      {onRename}
      {onSetDefault}
      {onReset}
      {onDelete}
    />
    {#if detail}
      {#key factsNonce}
        <ProfileEnrichmentBanner
          profileId={detail.profile.id}
          fields={detail.fields}
          {reviewing}
          onsave={() => void save()}
          onskip={reset}
        />
      {/key}
      <ProfileFieldsCard
        profileName={detail.profile.name}
        fields={detail.fields}
        {values}
        onchange={(key: string, value: string) => (values[key] = value)}
      />
      {#key factsNonce}
        <ProfileMemoryFactsCard
          profileId={detail.profile.id}
          profileName={detail.profile.name}
          facts={detail.facts}
        />
      {/key}
    {/if}
  </SettingsSubPage>
{/if}
