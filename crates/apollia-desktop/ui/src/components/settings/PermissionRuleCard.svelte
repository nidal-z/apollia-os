<script lang="ts">
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Trash2, ShieldCheck, ShieldX } from "lucide-svelte";
  import type {
    PermissionRuleDto,
    PermissionRuleScope,
  } from "$lib/stores/permissions";

  interface Props {
    rule: PermissionRuleDto;
    busy?: boolean;
    onRevoke: (rule: PermissionRuleDto) => void;
  }

  let { rule, busy = false, onRevoke }: Props = $props();

  const SCOPE_LABEL: Record<PermissionRuleScope, string> = {
    session: "Session en cours",
    project: "Ce projet",
    agent: "Chat",
    global: "Partout",
  };

  const SCOPE_VARIANT: Record<
    PermissionRuleScope,
    "warning" | "info" | "primary" | "secondary"
  > = {
    session: "warning",
    project: "info",
    agent: "secondary",
    global: "primary",
  };

  const isAllow = $derived(rule.action === "allow");

  function formatExpiration(value: string | null, scope: PermissionRuleScope): string {
    if (scope === "session") return "Expire à la fermeture · Non persisté";
    if (!value) return "Permanente";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return `Expire le ${value}`;
    return `Expire le ${date.toISOString().slice(0, 10)}`;
  }

  function formatCreated(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return `Créé le ${date.toISOString().slice(0, 10)}`;
  }

  const revokeLabel = $derived(
    rule.scope === "session" ? "Révoquer (session)" : "Révoquer",
  );
</script>

<article
  class="rounded-md border border-border bg-card px-4 py-3 shadow-sm"
  data-testid="permission-rule-card"
  data-scope={rule.scope}
  data-rule-id={rule.id}
>
  <header class="flex items-start justify-between gap-3">
    <div class="flex flex-wrap items-center gap-2">
      {#if isAllow}
        <ShieldCheck size={14} aria-hidden="true" class="text-success" />
      {:else}
        <ShieldX size={14} aria-hidden="true" class="text-destructive" />
      {/if}
      <span class="text-sm font-medium">{rule.tool_name}</span>
      <Badge variant={SCOPE_VARIANT[rule.scope]} data-testid="permission-rule-scope">
        {SCOPE_LABEL[rule.scope]}
      </Badge>
    </div>
    <Button
      variant="ghost"
      size="sm"
      class="h-7 px-2 text-[11px]"
      disabled={busy}
      onclick={() => onRevoke(rule)}
      data-testid="permission-rule-revoke"
    >
      <Trash2 size={12} class="mr-1" aria-hidden="true" />
      {revokeLabel}
    </Button>
  </header>

  <dl class="mt-2 space-y-0.5 text-[12px] text-muted-foreground">
    {#if rule.arg_prefix}
      <div>
        <span class="font-medium text-foreground">Préfixe :</span>
        <code class="ml-1 rounded bg-muted px-1 py-px text-[11px]">{rule.arg_prefix}</code>
      </div>
    {:else}
      <div class="italic">Toutes invocations autorisées</div>
    {/if}

    {#if rule.scope === "project" && rule.project_path}
      <div>
        <span class="font-medium text-foreground">Projet :</span>
        <code class="ml-1 rounded bg-muted px-1 py-px text-[11px]">{rule.project_path}</code>
      </div>
    {/if}

    {#if rule.scope === "agent" && rule.agent_id}
      <div>
        <span class="font-medium text-foreground">Agent :</span>
        <code class="ml-1 rounded bg-muted px-1 py-px text-[11px]">{rule.agent_id}</code>
      </div>
    {/if}

    <div>{formatExpiration(rule.expires_at, rule.scope)} · {formatCreated(rule.created_at)}</div>

    {#if rule.created_by}
      <div>
        <span class="font-medium text-foreground">Auteur :</span>
        {rule.created_by}
      </div>
    {/if}
  </dl>
</article>
