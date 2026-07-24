<script lang="ts">
  /**
   * Topbar user menu. Avatar/initials trigger → dropdown
   * with Settings, Keyboard shortcuts, Reset Onboarding, Help, About, Quit.
   *
   * NOTE: there is no dedicated `DropdownMenu` primitive in the codebase
   * yet - the shared `Popover` already supports the required semantics
   * (portal, transition, keyboard dismissal) and is used here.
   */
  import { t } from "svelte-i18n";
  import { UserRound, Settings, Keyboard, RotateCcw, LifeBuoy, BadgeInfo, LogOut } from "lucide-svelte";
  import { Popover } from "$lib/components/ui/popover";
  import { navigateToSettings } from "$lib/router";

  let open = $state(false);

  function close() {
    open = false;
  }

  function goSettings() {
    navigateToSettings();
    close();
  }

  function goShortcuts() {
    navigateToSettings("shortcuts");
    close();
  }

  function goDangerResetOnboarding() {
    navigateToSettings("danger");
    // The Danger page listens to ?focus=… to scroll + focus the card.
    // We can't set query string mid-app, so we rely on an inline anchor:
    // scroll happens via hash after sub-route mounts.
    setTimeout(() => {
      const el = document.getElementById("reset-onboarding");
      el?.scrollIntoView({ behavior: "smooth", block: "center" });
      (el?.querySelector("button") as HTMLButtonElement | null)?.focus();
    }, 150);
    close();
  }

  function goHelp() {
    navigateToSettings("help");
    close();
  }

  function goAbout() {
    navigateToSettings("about");
    close();
  }

  async function quit() {
    // Route through the `quit_app` command, not `window.close()`: closing the
    // window only hides it (close-to-tray). The command exits the process
    // gracefully via the shared quit path.
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("quit_app");
    } catch {
      // Web/dev fallback: no-op.
    }
  }
</script>

<Popover bind:open side="bottom" align="end" class="min-w-[220px] p-1">
  {#snippet trigger(triggerProps: Record<string, unknown>)}
    <button
      {...triggerProps}
      class="inline-flex h-10 w-10 items-center justify-center rounded-full border border-border bg-muted/50 text-foreground transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-label={$t('userMenu.open_label')}
      data-testid="topbar-user-menu"
    >
      <UserRound size={18} strokeWidth={1.75} />
    </button>
  {/snippet}
  {#snippet content()}
    <ul class="flex flex-col gap-0.5" role="menu" data-testid="topbar-user-menu-content">
      <li role="none">
        <button
          role="menuitem"
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground transition-colors hover:bg-muted"
          onclick={goSettings}
          data-testid="user-menu-settings"
        >
          <Settings size={14} strokeWidth={1.75} />
          <span>{$t('userMenu.settings')}</span>
        </button>
      </li>
      <li role="none">
        <button
          role="menuitem"
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground transition-colors hover:bg-muted"
          onclick={goShortcuts}
          data-testid="user-menu-shortcuts"
        >
          <Keyboard size={14} strokeWidth={1.75} />
          <span>{$t('userMenu.shortcuts')}</span>
        </button>
      </li>
      <li role="separator" aria-orientation="horizontal" class="my-1 h-px bg-border"></li>
      <li role="none">
        <button
          role="menuitem"
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground transition-colors hover:bg-muted"
          onclick={goDangerResetOnboarding}
          data-testid="user-menu-reset-onboarding"
        >
          <RotateCcw size={14} strokeWidth={1.75} />
          <span>{$t('userMenu.reset_onboarding')}</span>
        </button>
      </li>
      <li role="none">
        <button
          role="menuitem"
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground transition-colors hover:bg-muted"
          onclick={goHelp}
          data-testid="user-menu-help"
        >
          <LifeBuoy size={14} strokeWidth={1.75} />
          <span>{$t('userMenu.help')}</span>
        </button>
      </li>
      <li role="none">
        <button
          role="menuitem"
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm text-foreground transition-colors hover:bg-muted"
          onclick={goAbout}
          data-testid="user-menu-about"
        >
          <BadgeInfo size={14} strokeWidth={1.75} />
          <span>{$t('userMenu.about')}</span>
        </button>
      </li>
      <li role="separator" aria-orientation="horizontal" class="my-1 h-px bg-border"></li>
      <li role="none">
        <button
          role="menuitem"
          type="button"
          class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm text-destructive transition-colors hover:bg-destructive/10"
          onclick={quit}
          data-testid="user-menu-quit"
        >
          <LogOut size={14} strokeWidth={1.75} />
          <span>{$t('userMenu.quit')}</span>
        </button>
      </li>
    </ul>
  {/snippet}
</Popover>
