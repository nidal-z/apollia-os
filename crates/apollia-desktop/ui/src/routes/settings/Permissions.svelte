<script lang="ts">
  /**
   * Settings > Permissions - the authorization matrix.
   *
   * Thin orchestrator over the four permission surfaces, each a self-contained
   * section that owns its own load/reload lifecycle: persistent rules (with the
   * add form and bulk revoke), in-memory session authorizations, chat
   * "Always allow" rules, and the read-only audit tail. This page is the
   * primary consumer of `lib/errors/humanize` (see PermissionErrorBanner).
   */
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import RulesSection from "../../components/permissions/RulesSection.svelte";
  import SessionAuthSection from "../../components/permissions/SessionAuthSection.svelte";
  import ChatRulesSection from "../../components/permissions/ChatRulesSection.svelte";
  import AuditSection from "../../components/permissions/AuditSection.svelte";
</script>

<SettingsSubPage route="permissions" data-testid="settings-permissions">
  <RulesSection />
  <SessionAuthSection />
  <ChatRulesSection />
  <AuditSection />
</SettingsSubPage>
