<script lang="ts">
  /**
   * ProfileListCard - the profiles of the user: one row per profile, the
   * default marked, the displayed one selected, a menu per row (rename, set as
   * default, reset, delete) and "New profile". The page performs every change;
   * this card only gathers the user's intent through its dialogs.
   */
  import { t } from "svelte-i18n";
  import { Plus, UserRound } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import Dialog from "$lib/components/ui/dialog/Dialog.svelte";
  import DialogFooter from "$lib/components/ui/dialog/DialogFooter.svelte";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { cn } from "$lib/utils";
  import type { ProfileSummaryView } from "$lib/ipc/profile";
  import SettingsSection from "../SettingsSection.svelte";
  import ProfileResetDialog from "./ProfileResetDialog.svelte";
  import { profileMenu, type ProfileMenuAction, type ResetChoice } from "./profileForm";

  interface Props {
    profiles: ProfileSummaryView[];
    selectedId: string | null;
    onSelect: (id: string) => void;
    onCreate: (name: string) => Promise<boolean>;
    onRename: (profile: ProfileSummaryView, name: string) => Promise<boolean>;
    onSetDefault: (profile: ProfileSummaryView) => Promise<void>;
    onReset: (profile: ProfileSummaryView, choice: ResetChoice) => Promise<boolean>;
    onDelete: (profile: ProfileSummaryView) => Promise<boolean>;
  }

  let {
    profiles,
    selectedId,
    onSelect,
    onCreate,
    onRename,
    onSetDefault,
    onReset,
    onDelete,
  }: Props = $props();

  let createOpen = $state(false);
  let createName = $state("");
  let renaming = $state<ProfileSummaryView | null>(null);
  let renameName = $state("");
  let deleting = $state<ProfileSummaryView | null>(null);
  let resetting = $state<ProfileSummaryView | null>(null);
  let busy = $state(false);

  async function run(action: () => Promise<boolean>, close: () => void): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      if (await action()) close();
    } finally {
      busy = false;
    }
  }

  function choose(profile: ProfileSummaryView, action: ProfileMenuAction): void {
    if (action === "rename") {
      renaming = profile;
      renameName = profile.name;
    } else if (action === "set_default") {
      void onSetDefault(profile);
    } else if (action === "reset") {
      resetting = profile;
    } else {
      deleting = profile;
    }
  }
</script>

<SettingsSection
  title={$t("settings.profile.profiles.title")}
  description={$t("settings.profile.profiles.description")}
  bodyLayout="flush"
  data-testid="profile-list"
>
  <ul class="flex flex-col divide-y divide-border/60">
    {#each profiles as profile (profile.id)}
      {@const selected = profile.id === selectedId}
      <li
        class={cn(
          "flex items-center gap-3 rounded-lg px-2.5 py-2.5",
          selected && "bg-primary/10",
        )}
        data-testid="profile-row"
      >
        <button
          type="button"
          class="flex min-w-0 flex-1 items-center gap-3 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          aria-current={selected ? "true" : undefined}
          onclick={() => onSelect(profile.id)}
          data-testid="profile-row-select"
        >
          <UserRound size={15} strokeWidth={1.75} class="shrink-0 text-muted-foreground" />
          <span class="flex min-w-0 flex-col">
            <span class="truncate text-body-sm font-medium text-foreground" data-testid="profile-row-name"
              >{profile.name}</span
            >
            {#if profile.detail}
              <span class="truncate text-caption text-muted-foreground">{profile.detail}</span>
            {/if}
          </span>
        </button>
        {#if profile.is_default}
          <Badge variant="primary" outline data-testid="profile-row-default">
            {$t("settings.profile.profiles.default_badge")}
          </Badge>
        {/if}
        <ActionMenu
          triggerLabel={$t("settings.profile.profiles.actions", { values: { name: profile.name } })}
          data-testid="profile-row-menu"
        >
          {#snippet body({ close })}
            <ul class="flex w-64 flex-col gap-0.5" role="menu">
              {#each profileMenu(profile) as entry (entry.action)}
                <li role="none">
                  <button
                    type="button"
                    role="menuitem"
                    disabled={entry.disabled}
                    class={cn(
                      "flex w-full rounded-sm px-2 py-1.5 text-left text-body-xs transition-colors",
                      entry.destructive
                        ? "text-destructive hover:bg-destructive/10"
                        : "text-foreground hover:bg-muted",
                      entry.disabled && "cursor-not-allowed text-muted-foreground opacity-60",
                    )}
                    onclick={() => {
                      close();
                      choose(profile, entry.action);
                    }}
                    data-testid={`profile-menu-${entry.action}`}
                  >
                    {$t(entry.labelKey)}
                  </button>
                  {#if entry.reasonKey}
                    <p
                      class="px-2 pb-1.5 text-caption text-muted-foreground"
                      data-testid="profile-menu-delete-reason"
                    >
                      {$t(entry.reasonKey)}
                    </p>
                  {/if}
                </li>
              {/each}
            </ul>
          {/snippet}
        </ActionMenu>
      </li>
    {/each}
  </ul>
  <div class="pt-3">
    <Button
      variant="outline"
      size="sm"
      onclick={() => {
        createName = "";
        createOpen = true;
      }}
      data-testid="profile-new"
    >
      {#snippet icon()}
        <Plus size={14} strokeWidth={2} />
      {/snippet}
      {$t("settings.profile.profiles.new")}
    </Button>
  </div>
</SettingsSection>

<Dialog
  open={createOpen}
  onclose={() => (createOpen = false)}
  size="sm"
  title={$t("settings.profile.create_dialog.title")}
  data-testid="profile-create-dialog"
>
  <Input
    bind:value={createName}
    aria-label={$t("settings.profile.create_dialog.label")}
    placeholder={$t("settings.profile.create_dialog.placeholder")}
    onkeydown={(e: KeyboardEvent) => {
      if (e.key === "Enter" && createName.trim())
        void run(() => onCreate(createName.trim()), () => (createOpen = false));
    }}
    data-testid="profile-create-name"
  />
  <DialogFooter>
    <Button variant="outline" onclick={() => (createOpen = false)} data-testid="profile-create-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      disabled={!createName.trim()}
      loading={busy}
      onclick={() => void run(() => onCreate(createName.trim()), () => (createOpen = false))}
      data-testid="profile-create-confirm"
    >
      {$t("settings.profile.create_dialog.confirm")}
    </Button>
  </DialogFooter>
</Dialog>

<Dialog
  open={renaming !== null}
  onclose={() => (renaming = null)}
  size="sm"
  title={$t("settings.profile.rename_dialog.title", { values: { name: renaming?.name ?? "" } })}
  data-testid="profile-rename-dialog"
>
  <Input
    bind:value={renameName}
    aria-label={$t("settings.profile.create_dialog.label")}
    data-testid="profile-rename-name"
  />
  <DialogFooter>
    <Button variant="outline" onclick={() => (renaming = null)} data-testid="profile-rename-cancel">
      {$t("common.cancel")}
    </Button>
    <Button
      disabled={!renameName.trim()}
      loading={busy}
      onclick={() => {
        const target = renaming;
        if (target) void run(() => onRename(target, renameName.trim()), () => (renaming = null));
      }}
      data-testid="profile-rename-confirm"
    >
      {$t("settings.profile.rename_dialog.confirm")}
    </Button>
  </DialogFooter>
</Dialog>

<ConfirmDialog
  open={deleting !== null}
  onclose={() => (deleting = null)}
  onconfirm={() => {
    const target = deleting;
    if (target) void run(() => onDelete(target), () => (deleting = null));
  }}
  title={$t("settings.profile.delete_dialog.title", { values: { name: deleting?.name ?? "" } })}
  message={$t("settings.profile.delete_dialog.message")}
  confirmLabel={$t("settings.profile.delete_dialog.confirm")}
  loading={busy}
  data-testid="profile-delete-dialog"
/>

<ProfileResetDialog
  profile={resetting}
  {busy}
  onclose={() => (resetting = null)}
  onconfirm={(choice: ResetChoice) => {
    const target = resetting;
    if (target) void run(() => onReset(target, choice), () => (resetting = null));
  }}
/>
