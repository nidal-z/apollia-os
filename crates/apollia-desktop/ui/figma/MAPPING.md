# Figma <-> Code mapping, Apollia OS Design System

Version-controlled provenance for the Figma file **"Apollia OS Design System"**.
File: https://www.figma.com/design/2TLZ2uqIOweX14eP4VGXHq
Generated as a code-faithful mirror. Source paths are relative to `crates/apollia-desktop/ui/src/`.

Open a node directly: append `?node-id=<NODE>` to the file URL (hyphen form, e.g. `node-id=23-2`).

## Primitives (lib/components/ui)

| Figma component | node-id | Source |
|---|---|---|
| Button | 23:2 | lib/components/ui/button/Button.svelte |
| Badge | 25:2 | lib/components/ui/badge/Badge.svelte |
| Input | 27:47 | lib/components/ui/input/Input.svelte |
| Select | 28:29 | lib/components/ui/select/Select.svelte |
| Checkbox | 29:10 | lib/components/ui/checkbox/Checkbox.svelte |
| Toggle | 30:24 | lib/components/ui/toggle/Toggle.svelte |
| Card | 35:26 | lib/components/ui/card/Card.svelte |
| Avatar | 36:22 | lib/components/ui/avatar/Avatar.svelte |
| Separator | 37:26 | lib/components/ui/separator/Separator.svelte |
| TabBar | 39:16 | lib/components/ui/tabs/TabBar.svelte |
| Banner | 40:32 | lib/components/ui/banner/Banner.svelte |
| Dialog | 45:26 | lib/components/ui/dialog/Dialog.svelte |
| Sheet | 46:38 | lib/components/ui/sheet/Sheet.svelte |
| Tooltip | 47:18 | lib/components/ui/tooltip/Tooltip.svelte |
| ProgressBar | 48:42 | lib/components/ui/progress/ProgressBar.svelte |
| Spinner | 49:8 | lib/components/ui/progress/Spinner.svelte |
| Skeleton | 50:7 | lib/components/ui/skeleton/Skeleton.svelte |
| Stepper | 51:36 | lib/components/ui/stepper/Stepper.svelte |
| Popover | 52:38 | lib/components/ui/popover/Popover.svelte |
| Textarea | 54:20 | lib/components/ui/textarea/Textarea.svelte |
| FormField | 55:32 | lib/components/ui/form-field/FormField.svelte |
| Radio | 56:20 | lib/components/ui/radio/RadioItem.svelte |
| Breadcrumbs | 57:15 | lib/components/ui/breadcrumbs/Breadcrumbs.svelte |
| Accordion | 57:26 | lib/components/ui/accordion/AccordionItem.svelte |
| Combobox | 58:24 | lib/components/ui/combobox/Combobox.svelte |
| DataTable | 59:92 | lib/components/ui/data-table/DataTable.svelte |
| Command | 60:44 | lib/components/ui/command/Command.svelte |
| Icon | 61:20 | lib/components/ui/icon/Icon.svelte |
| GlossaryTerm | 61:21 | lib/components/ui/tooltip/GlossaryTerm.svelte |
| MarkdownContent | 61:23 | lib/components/ui/markdown/MarkdownContent.svelte |
| ActionMenu | 62:19 | lib/components/ui/action-menu/ActionMenu.svelte |
| ConfirmDialog | 62:47 | lib/components/ui/dialog/ConfirmDialog.svelte |
| DatePicker | 63:90 | lib/components/ui/date-picker/DatePicker.svelte |
| TimePicker | 63:108 | lib/components/ui/date-picker/TimePicker.svelte |

## Feature components (lib/components)

> Coverage expanded to **126 feature component sets** across the 5 Features pages
> (Chat & Plan 47, Inbox/Tasks/Agents 25, Memory/Connections/Settings 25,
> Observability 15, Layout & Common 14). The table below lists the first wave with
> node-ids; the additional sets were generated light-first with the same pattern,
> their node-ids visible in-file and on the 🔍 Audit page. The whole file is now
> light-first (white mode); flip the Color collection to Dark for dark mode.

| Figma component | node-id | Source |
|---|---|---|
| StatusDot | 64:14 | lib/components/operator/StatusDot.svelte |
| PageHeader | 64:15 | lib/components/operator/PageHeader.svelte |
| SectionTitle | 64:23 | lib/components/operator/SectionTitle.svelte |
| EmptyState | 64:63 | lib/components/operator/EmptyState.svelte |
| AppSidebar | 65:88 | lib/components/app/Sidebar.svelte |
| Topbar | 70:2 | lib/components/app/Topbar.svelte |
| ModeChip | 70:38 | lib/components/app/ModeChip.svelte |
| UserMenu | 70:52 | lib/components/app/UserMenu.svelte |
| ChatMessageBubble | 71:10 | lib/components/chat/ChatMessageBubble.svelte |
| StreamingMessage | 71:11 | lib/components/chat/StreamingMessage.svelte |
| ThinkingBadge | 71:15 | lib/components/chat/ThinkingBadge.svelte |
| ReasoningCard | 71:40 | lib/components/chat/ReasoningCard.svelte |
| ChatInput | 71:41 | lib/components/chat/ChatInput.svelte |
| ApprovalCard | 75:41 | lib/components/chat/OperatorApprovalCard.svelte |
| AgentStatusCard | 75:42 | lib/components/chat/AgentStatusCard.svelte |
| PlanStepNode | 75:81 | lib/components/operator/PlanStepNode.svelte |
| PlanDagPanel | 75:82 | lib/components/operator/PlanDagPanel.svelte |
| TaskRow | 76:61 | lib/components/operator/TaskRow.svelte |
| ProjectCard | 76:115 | lib/components/operator/ProjectCard.svelte |
| ConversationRow | 77:31 | lib/components/operator/ConversationRow.svelte |
| ActivityRow | 77:32 | lib/components/operator/ActivityRow.svelte |
| InboxRow | 77:66 | lib/components/operator/InboxRow.svelte |
| HITLCard | 78:2 | lib/components/operator/HITLCard.svelte |
| AgentPackageCard | 78:17 | lib/components/agents/AgentPackageCard.svelte |
| TaskTimeline | 78:34 | lib/components/operator/TaskTimeline.svelte |
| MemoryEntryRow | 79:29 | lib/components/memory/MemoryEntryRow.svelte |
| SettingsToggle | 79:42 | lib/components/settings/SettingsToggle.svelte |
| SettingsSection | 79:43 | lib/components/settings/SettingsSection.svelte |
| ConnectionStatusIndicator | 79:67 | lib/components/connections/ConnectionStatusIndicator.svelte |
| ConnectorWizard | 80:2 | lib/components/connections/ConnectorWizard.svelte |
| PermissionView | 80:56 | lib/components/chat/permission/PermissionDispatcher.svelte |
| OnboardingStep | 80:57 | lib/components/chat/onboarding/OnboardingModal.svelte |
| AuditTrailTable | 81:2 | lib/components/observability/AuditTrailTable.svelte |
| ToolAuditRow | 81:53 | lib/components/observability/ToolAuditRow.svelte |
| HookDecisionLog | 81:82 | lib/components/observability/HookDecisionLog.svelte |
| TraceEventCard | 82:2 | lib/components/observability/TraceEventCard.svelte |
| LlmCostChart | 82:24 | lib/components/observability/LlmCostChart.svelte |
| DelegationTree | 82:52 | lib/components/observability/DelegationTree.svelte |

## Layout primitives (code refonte, synced to Figma 2026-06-15)

> Added during the code-side consolidation and mirrored on the 🧩 Primitives page
> under the "OPERATOR LAYOUT PRIMITIVES" column (x=10800), bound to the Color
> tokens (light-first). ListRow is a component set with a `State` property; the
> other three are single components.

| Component | node-id | Source | Role |
|---|---|---|---|
| ListRow | 140:26 | lib/components/operator/ListRow.svelte | Canonical clickable row shell (state bg default/active/unread/dimmed, padding, hairline, button role + keyboard). |
| ListPanel | 143:2 | lib/components/operator/ListPanel.svelte | Bordered list/table container (`rounded-xl border bg-card`), optional column header. |
| FilterChipBar | 142:2 | lib/components/operator/FilterChipBar.svelte | Canonical status/filter chip row, `default` + `compact` sizes, optional `rightSlot`. |
| SplitLayout | 145:2 | lib/components/operator/SplitLayout.svelte | Sidebar (280px) + detail route shell. |

### Refonte verdicts applied in code

- **ListRow** now backs `TaskRow`, `InboxRow`, `ConversationRow` (thin wrappers, pixel-identical).
- **ApprovalRiskBadge** is now a thin wrapper over `RiskBadge` (`kind="approval"`); the level -> icon/color/label map lives once in `RiskBadge`.
- **FilterChipBar** adopted in `Tasks` (page-level 1:1 + compact sidebar), `Automations` and `Inbox` (pending + activity); the prior hand-rolled chip rows are gone. This unified the slightly divergent looks onto the canonical Tasks treatment.
- **ListPanel** adopted in `Tasks` and `Automations` list/table wrappers.
- **SplitLayout** adopted in `Projects` (240 -> 280) and `Connections` (300 -> 280); sidebar width standardized to 280. `Agents` (section uses `overflow-y-auto`) and `Settings` (nav rail, `bg-muted/30`) were intentionally left as-is.
- **Not changed** (kept for 1:1 safety): `operator/Card` (a11y role/keyboard), the 19 `EmptyState` call-sites, inline avatars.

## Chantier 1 primitives — header system (étape 3, 2026-06-16)

> Canonical header/navigation primitives validated in étape 3 (co-construction).
> Rule: mono-column screens use `PageHeader`; split screens use breadcrumb +
> `DetailHeader` (no full page header). Kicker is contextual-only (date/count/
> status), never a static label that repeats the title. To be implemented in
> code at étape 4 and rolled out across screens.

| Component | node-id | Source (target) | Role |
|---|---|---|---|
| Tab | 237:27 | lib/components/ui/tabs (Tab) | Atomic underline tab (State active/inactive + `count` layer). A `TabBar` is an auto-layout of N `Tab` → dynamic tab count. |
| PageHeader | 238:2 | lib/components/operator/PageHeader.svelte | `kicker` (optional) + title + subtitle + `actions` (1-N buttons). Replaces the single-action PageHeader. |
| DetailHeader | 238:12 | lib/components/operator/DetailHeader.svelte (new) | `leading` (icon/avatar) + title + `badge` + meta + `actions`. Unifies the per-screen detail headers (Connections/Projects/Agents/Tasks-split/Memory). |

Étape-3 proposal frames (Templates page, "Proposals · Chantier 1"): Memory
(`233:1962`, split → breadcrumb + DetailHeader) and Tasks (`235:2099`, mono →
PageHeader without kicker + 2 actions).

## Chantier 2 primitives — lists & sidebars (étape 4, 2026-06-16)

> Canonical sidebar primitives. Implemented in code and mirrored in Figma.

| Component | Source | Role |
|---|---|---|
| SidebarHeader | lib/components/operator/SidebarHeader.svelte | Split-sidebar top: `title` + `count` + `actions` + optional `search` + `filters`. Adopted in Memory, Projects, Connections, Agents, Tasks split. |
| ListRow (`nav`) | lib/components/operator/ListRow.svelte | New `nav` variant: rounded pill, no hairline, rounded state bg. Routes the hand-rolled sidebar rows (Memory namespaces, Projects, Connections native + MCP, Tasks split). Agents rows deferred (bespoke hover actions). |

Figma sync: Memory sidebar background switched from `surface-1/40` (cream) to
plain `background`; the Tasks-split sidebar heading was normalised from the mono
uppercase label to the standard SidebarHeader title.

## Chantier 3 primitives — states & errors (étape 4, 2026-06-16)

> Canonical loading and error surfaces. The error convention is: a banner at the
> top of the content for persistent/blocking errors (load failures), toasts for
> transient action feedback. The runtime-disconnected banner is now rendered once
> globally in `app/Main.svelte` (above the routed content), not per route.

| Component | node-id | Source | Role |
|---|---|---|---|
| ErrorBanner | 245:2 | lib/components/operator/ErrorBanner.svelte (new) | Inline alert: `message` + `tone` (danger/warning/info) + optional `onretry`/`ondismiss`. Replaces ~17 hand-rolled `border-destructive` error divs across routes and the Settings sub-routes. Tint/border/text bound to `semantic/*` + `text/*` tokens (tint at 6%, border at 32%). |
| SkeletonList | 247:2 | lib/components/operator/SkeletonList.svelte (new) | List/sidebar loading placeholder: `count` rows, optional leading `avatar`. Bound to `surface/muted`. Backs the Agents sidebar loading state. |

Figma sync: these are conditional (error) and transient (loading) states, not
shown in the default-state route templates, so the template frames are unchanged.
ErrorBanner (`245:2`) and SkeletonList (`247:2`) are added to the Primitives
page (`1:3`) as token-bound library showcases.

## Topbar breadcrumb (synced to code 2026-06-16)

> The Topbar component (`70:2`) breadcrumb now mirrors `OperatorBreadcrumb`:
> separator is `/` (was `›`), and a route-specific primary icon precedes the
> last segment. The icon is a new variant set `BreadcrumbRouteIcon` instanced
> in the master breadcrumb and switched per template via its `Route` property.

| Component | node-id | Source | Role |
|---|---|---|---|
| BreadcrumbRouteIcon | 157:61 | lib/components/layout/OperatorBreadcrumb.svelte (route icon) | Per-route lucide glyph (primary) for the Topbar breadcrumb; `Route` variant matches `routeMeta` (dashboard..automations). Glyphs reused from AppSidebar, bound to `brand/primary`. |

## Route templates (📐 Templates page)

| Route | Source |
|---|---|
| Dashboard | routes/Dashboard.svelte (App shell = app/Sidebar + app/Topbar) |
| Chat | routes/Chat.svelte (App shell = app/Sidebar + app/Topbar) |
| Tasks | routes/Tasks.svelte (App shell = app/Sidebar + app/Topbar) |
| Inbox | routes/Inbox.svelte (App shell = app/Sidebar + app/Topbar) |
| Memory | routes/Memory.svelte (App shell = app/Sidebar + app/Topbar) |
| Settings | routes/Settings.svelte (App shell = app/Sidebar + app/Topbar) |
| Agents | routes/Agents.svelte (App shell = app/Sidebar + app/Topbar) |
| Projects | routes/Projects.svelte (App shell = app/Sidebar + app/Topbar) |
| Connections | routes/Connections.svelte (App shell = app/Sidebar + app/Topbar) |
| Notifications | routes/Notifications.svelte (App shell = app/Sidebar + app/Topbar) |
| Automations | routes/Automations.svelte (App shell = app/Sidebar + app/Topbar) |
| Tasks (split) | routes/Tasks.svelte split mode (second template frame) |
| Llm | routes/Llm.svelte (App shell) |
| Observability | routes/Observability.svelte (App shell) |
| Transcriptions | routes/Transcriptions.svelte (App shell) |
| SettingsPermissionRules | routes/SettingsPermissionRules.svelte (App shell) |
| Settings sub-routes (×13) | routes/settings/*.svelte (content panes, grouped in the "Settings · sub-routes" frame) |

> Integrations is no longer a standalone route template. The nav route
> `integrations` renders `routes/Connections.svelte` (native connectors + MCP);
> the legacy `routes/Integrations.svelte` is not wired in `app/Main.svelte`. The
> Figma "Template · Integrations" frame was removed (2026-06-16).

### Template resync log (Code -> Figma, 2026-06-16)

> Post-refacto resync of the route templates to mirror the operator-primitive
> code. Each entry validated Light + Dark.

- **Memory** (`87:574`): rebuilt body to match `routes/Memory.svelte`. Added
  `PageHeader`; categorized `NamespaceSidebar` (search + category segmented
  filter + grouped items) on `SplitLayout` (280, `bg-surface-1/40`); main header
  (namespace breadcrumb + `MemorySearch`); underline `TabBar`
  (all/episodic/semantic/procedural); `MemoryEntryRow` instances. Sidebar/search
  composed inline from primitives (no `NamespaceSidebar`/`MemorySearch` Figma
  component yet).
- **Tasks** (`86:343` list, `188:1973` split): list mode rebuilt to match
  `routes/Tasks.svelte` (PageHeader `MY WORK`; canonical `FilterChipBar` 6 chips
  all/todo/running/approval/done/error with status dots + counts; `ListPanel`
  card with column header + `TaskRow`). Added a second frame **Template · Tasks
  (split)** for the split mode (`SplitLayout`: compact `FilterChipBar` + task
  list + back; detail header avatar/badge/meta + underline `TabBar`
  overview/output/trace + overview cards). TaskRow sample badges/relative-time
  remain FR (pre-refacto component, not retouched). PageHeader exposes a single
  action slot, so the secondary Refresh button is not shown.

### Figma binding gotchas (learned 2026-06-16)

> Two pitfalls when binding color variables via `use_figma`:
> 1. Spreading a bound paint (`{...p, opacity}`) can drop variable resolution
>    (node renders the literal base color). Bind with
>    `setBoundVariableForPaint` and set opacity afterwards by deep-cloning the
>    fills/strokes array (`JSON.parse(JSON.stringify(arr))`, set `[0].opacity`,
>    reassign). Bind AFTER the node is parented to a resolving frame.
> 2. Some pre-refacto nodes have a broken/phantom Color-mode resolution (fill
>    renders black despite a valid binding). Recreate the container as a fresh
>    frame so it follows mode inheritance (and flips Light/Dark correctly).
> 3. Toggling a frame's explicit Color mode (for Dark preview) RE-RESOLVES paints
>    and resets any custom `opacity` back to 1. Apply translucent opacities as the
>    last step and avoid per-frame mode toggling afterward.
> 4. `opacity` on a paint bound to a semantic color variable (success/warning/
>    destructive/info) is ignored by the renderer (badge renders solid). Status
>    badges are therefore styled as a colored dot + colored text with NO fill,
>    instead of a tinted pill.

- **Connections** (`89:1004`): rebuilt to `routes/Connections.svelte` (route
  `integrations`). Sidebar = search + NATIVE connectors (Google/Microsoft, status
  dots) + MCP SERVERS (health dots); detail = connector header + Accounts/
  Capabilities/Settings tabs + connected-account card with scope chips. Composed
  inline from primitives.
- **Projects** (`89:869`): rebuilt to `routes/Projects.svelte`. Sidebar = title +
  count + new, search, project list with color bars; detail = folder header +
  agent chips + Settings/Open-chat + underline `TabBar`
  conversations/tasks/memory/context/settings + conversation rows. NewProjectDialog
  is an overlay with no Figma component yet (candidate new component, out of the
  template scope).
- **Automations** (`89:1221`): rebuilt to `routes/Automations.svelte`. PageHeader
  (action `New automation`) + `FilterChipBar` (all/active/paused/error) + ListPanel
  card with column header + trigger rows (name, cron schedule, status, last run,
  toggle).
- **Inbox** (`86:485`): `routes/Inbox.svelte`. `TabBar` pending/activity/
  notifications (counts) + pending `FilterChipBar` (all/approval/ask_user) + agent
  Select rightSlot + `Today` section + InboxRow.
- **Settings** (`87:691`): nav rail rebuilt to the grouped clusters of
  `routes/Settings.svelte` (Personalization / AI / System / Danger, `bg-muted/30`,
  240px), NOT `SplitLayout`. The "Settings filter on FilterChipBar" note does not
  apply to the top-level Settings shell (it has no FilterChipBar); any such filter
  lives in a specific sub-route, out of this template's scope.

### Completeness pass (2026-06-16, post-review)

> Added so the Figma file covers every product route for the étape-2 audit.

- **Dashboard** (`83:2`): rebuilt to the bento layout of `routes/Dashboard.svelte`
  (pending decisions + deliverables + at-work cards, recent activity strip).
- **Chat** (`85:183`): rebuilt to the 3-panel layout (conversations sidebar |
  thread with bubbles + tool approval + composer | Journal/Plan rail).
- **Agents** (`87:809`): rebuilt to My assistants / My packages sidebar +
  detail (status badges + Overview/Tools/Memory/Activity/Settings tabs).
- **Notifications** (`89:1112`): rebuilt to notified-events + channels grid +
  history notice (the old Overview/Activity/Settings tabs were obsolete).
- **Llm** / **Observability** / **Transcriptions**: new route templates. Added
  `BreadcrumbRouteIcon` variants `llm` (Brain), `observability` (Activity),
  `transcriptions` (Mic) to the set (`157:61`).
- **SettingsPermissionRules** (`230:1849`): new full template (filter chips +
  always-accept rules table). Breadcrumb uses the `settings` route icon.
- **Settings sub-routes**: the 13 panes (`routes/settings/*.svelte`) are
  represented as content-only panes grouped in the "Settings · sub-routes" frame
  (not full app-shell frames, since the shell is identical to the Settings
  template). Representative-fidelity (heading + key sections per pane).

Coverage is now 15 standalone route templates + Tasks split frame + 13 Settings
content panes. Templates use sample data; selection tints rely on paint opacity
(see Dark gotcha #3). Pre-refacto feature components (TaskRow, MemoryEntryRow,
InboxRow) keep their FR sample text.

## Tokens (🎨 Tokens page)

| Figma | Source |
|---|---|
| Color variables (Light/Dark modes) | src/app.css , `:root` + `.dark` (HSL) |
| Radius / Spacing variables | src/app.css `--radius` + tailwind.config.ts |
| Text styles (Inter / JetBrains Mono / Instrument Serif) | tailwind.config.ts `fontFamily` + `fontSize` |
| Effect styles (elevation, primary, status shadows) | src/app.css `--shadow-*` |

_Note: Inter Tight (code default sans) is not installed locally; the Figma text styles fall back to Inter, matching the code's font stack._
