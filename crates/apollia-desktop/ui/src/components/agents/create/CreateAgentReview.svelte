<script lang="ts">
  /**
   * CreateAgentReview - the draft read over in the form of an agent sheet:
   * the name, then the same sections an existing agent is adjusted with
   * (mission and examples, model, accesses, sources). The form belongs to the
   * creation; nothing is written until "Save the agent".
   *
   * Every access of the draft is listed. One of a sensitive family opens
   * unchecked and says so; one added because an agent created without code is
   * granted it by default says so too.
   */
  import { t } from "svelte-i18n";
  import { llmBackends } from "$lib/stores/llm";
  import { describeTool, listTools, toolKindTag } from "$lib/ipc/tools";
  import { listMcpExposable } from "$lib/ipc/mcpServer";
  import { listConnections } from "$lib/ipc/connections";
  import { listModelPresets } from "$lib/ipc/agentAdjust";
  import { sortConnections } from "$lib/connections/connectionList";
  import { Banner } from "$lib/components/ui/banner";
  import { FormField } from "$lib/components/ui/form-field";
  import { Input } from "$lib/components/ui/input";
  import type { Connection } from "$lib/types";
  import AdjustMission from "../sheet/AdjustMission.svelte";
  import AdjustModel from "../sheet/AdjustModel.svelte";
  import AdjustAccess from "../sheet/AdjustAccess.svelte";
  import AdjustSources from "../sheet/AdjustSources.svelte";
  import {
    grantCandidates,
    modelOptions,
    type AdjustDraft,
    type GrantCandidate,
    type PresetOffer,
  } from "../sheet/agentAdjust";
  import { connectionToolFamily, remoteAccesses, type ConnectionOffer } from "../sheet/connectionGrant";
  import { accessView, groupAccesses, type AccessDescriptor, type AccessView } from "../sheet/agentSheet";
  import { ACCESS_NOTE_LABEL, type AccessNote } from "./agentDraft";

  interface Props {
    /** Display name, edited here. */
    name: string;
    /** The form of the creation; the same object the save reads. */
    form: AdjustDraft;
    /** Every access of the draft, checked or not. */
    listed: string[];
    /** The accesses and the sources checked when the form opened. */
    checked: string[];
    sources: string[];
    notes: Record<string, AccessNote>;
  }

  let { name = $bindable(), form, listed, checked, sources, notes }: Props = $props();

  let presets = $state<PresetOffer[]>([]);
  let connections = $state<Connection[]>([]);
  let views = $state<AccessView[]>([]);

  $effect(() => {
    let live = true;
    void listModelPresets()
      .then((list) => {
        if (live) presets = list;
      })
      .catch(() => {
        if (live) presets = [];
      });
    void listConnections()
      .then((list) => {
        if (live) connections = list;
      })
      .catch(() => {
        if (live) connections = [];
      });
    return () => {
      live = false;
    };
  });

  async function describeAccess(id: string): Promise<AccessView> {
    if (id.startsWith("agent:") || id.startsWith("workflow:")) return accessView(id, null);
    if (id.startsWith("mcp:")) {
      const family = connectionToolFamily(id, connections);
      return accessView(id, family ? { description: "", family, family_source: "server", kind: "mcp" } : null);
    }
    const descriptor = await describeTool(id).catch(() => null);
    const read: AccessDescriptor | null = descriptor
      ? {
          description: descriptor.description,
          family: descriptor.family,
          family_source: descriptor.family_source,
          kind: toolKindTag(descriptor.kind),
        }
      : null;
    return accessView(id, read);
  }

  // The accesses of the draft, then those granted in the form, each described
  // once so it lands in its family.
  $effect(() => {
    const wanted = [...new Set([...listed, ...form.access])];
    const known = new Set(views.map((v) => v.id));
    const missing = wanted.filter((id) => !known.has(id));
    if (missing.length === 0) return;
    let live = true;
    void Promise.all(missing.map((id) => describeAccess(id))).then((described) => {
      if (live) views = [...views, ...described];
    });
    return () => {
      live = false;
    };
  });

  const groups = $derived.by(() => {
    const byId = new Map(views.map((v) => [v.id, v]));
    const ids = [...new Set([...listed, ...form.access])];
    return groupAccesses(ids.map((id) => byId.get(id) ?? accessView(id, null)));
  });

  const badges = $derived(
    Object.fromEntries(
      Object.entries(notes).map(([id, note]) => [
        id,
        { label: ACCESS_NOTE_LABEL[note], tone: note === "sensitive" ? ("warning" as const) : ("neutral" as const) },
      ]),
    ),
  );

  const remote = $derived(remoteAccesses(views.map((v) => v.id), connections));

  async function loadCandidates(): Promise<GrantCandidate[]> {
    const [tools, targets] = await Promise.all([
      listTools().catch(() => []),
      listMcpExposable().catch(() => []),
    ]);
    return grantCandidates(tools, targets, form.access, "");
  }

  async function loadConnectionOffer(): Promise<ConnectionOffer> {
    const [connected, tools] = await Promise.all([
      listConnections().catch(() => [] as Connection[]),
      listTools().catch(() => []),
    ]);
    connections = connected;
    return {
      connections: sortConnections(connected.filter((c) => c.tools.length > 0)),
      descriptions: new Map(tools.map((tool) => [tool.name, tool.description])),
    };
  }
</script>

<div class="max-w-3xl space-y-6" data-testid="agent-create-review" aria-label={$t("agent_create.review_title")}>
  <Banner variant="info" surface="card" data-testid="agent-create-review-notice">
    {$t("agent_create.review_notice")}
  </Banner>
  <FormField
    label={$t("agent_create.name_label")}
    id="agent-create-name"
    hint={$t("agent_create.name_help")}
    required
  >
    <Input
      id="agent-create-name"
      bind:value={name}
      placeholder={$t("agent_create.name_placeholder")}
      data-testid="agent-create-name"
    />
  </FormField>
  <AdjustMission draft={form} modified={false} />
  <AdjustModel draft={form} options={modelOptions($llmBackends)} {presets} modified={false} />
  <AdjustAccess
    draft={form}
    {groups}
    inForce={checked}
    modified={false}
    {loadCandidates}
    agentName={name}
    {loadConnectionOffer}
    {remote}
    notes={badges}
  />
  <AdjustSources draft={form} inForce={sources} modified={false} />
</div>
