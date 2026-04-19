<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fly, slide } from "svelte/transition";
  import { t } from "svelte-i18n";
  import type { TimelineEvent } from "$lib/types";
  import { formatRelativeTime } from "$lib/utils";
  import { Badge } from "$lib/components/ui/badge";
  import { uiMode } from "$lib/stores/mode";
  import {
    RefreshCw, ChevronDown, ChevronRight, Brain, Zap,
    Terminal, Code, Globe, FileText, FilePen, FilePenLine,
    FolderOpen, Search, FileSearch, Plug,
  } from "lucide-svelte";
  import type { ComponentType } from "svelte";

  interface Props {
    taskId: string;
    isRunning: boolean;
  }

  let { taskId, isRunning }: Props = $props();

  let events = $state<TimelineEvent[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let expandedTools = $state<Set<number>>(new Set());
  let expandedObservations = $state<Set<number>>(new Set());

  const POLL_INTERVAL_MS = 2000;
  const OBSERVATION_PREVIEW_LENGTH = 80;

  const STATUS_I18N: Record<string, string> = {
    working: "dashboard.status_working",
    submitted: "dashboard.status_submitted",
    completed: "dashboard.status_completed",
    failed: "dashboard.status_failed",
    input_required: "dashboard.status_approval",
    canceled: "dashboard.status_canceled",
  };

  const EVENT_DOT_COLOR: Record<string, string> = {
    task_transition: "bg-primary",
    step_started: "bg-info",
    step_completed: "bg-success",
    llm_call: "bg-secondary",
    tool_call: "bg-muted-foreground",
    hitl_suspended: "bg-warning",
    hitl_resolved: "bg-success",
    task_completed: "bg-success",
    step_observation: "bg-secondary",
    plan_cache_hit: "bg-primary",
  };

  const MODEL_BADGE_COLORS: Record<string, string> = {
    "gpt-4o": "bg-primary/20 text-primary",
    "gpt-4o-mini": "bg-primary/15 text-primary/80",
    "haiku": "bg-secondary/20 text-secondary",
    "sonnet": "bg-secondary/20 text-secondary",
    "opus": "bg-secondary/25 text-secondary",
  };

  const DEFAULT_MODEL_BADGE = "bg-muted text-muted-foreground";

  const EXECUTION_MODE_STYLES: Record<string, { operator: string; builder: string; color: string }> = {
    direct: {
      operator: "Simple",
      builder: "Direct",
      color: "bg-primary/20 text-primary",
    },
    orchestrated: {
      operator: "Complex",
      builder: "Orchestrated",
      color: "bg-secondary/20 text-secondary",
    },
  };

  // Tool icon mapping
  const TOOL_ICONS: Record<string, ComponentType> = {
    file_read: FileText,
    file_write: FilePen,
    file_edit: FilePenLine,
    file_list: FolderOpen,
    file_glob: Search,
    file_grep: FileSearch,
    bash_executor: Terminal,
    python_executor: Code,
    http_fetch: Globe,
    memory_search: Brain,
    memory_store: Brain,
  };

  function getToolIcon(name: string): ComponentType {
    if (TOOL_ICONS[name]) return TOOL_ICONS[name];
    if (name.startsWith("mcp:")) return Plug;
    return Terminal;
  }

  /** i18n label key for a tool name */
  function getToolLabelKey(name: string): string {
    if (name.startsWith("mcp:")) return "tools.labels.mcp_tool";
    return `tools.labels.${name}`;
  }

  /** Extracts the last path segment (filename) from a shell argument. */
  function shellFilename(arg: string): string {
    return arg.replace(/^['"]|['"]$/g, "").split(/[/\\]/).filter(Boolean).pop() ?? arg;
  }

  /** Extracts the first non-flag argument from a token list. */
  function firstPath(tokens: string[]): string | null {
    const p = tokens.find((t) => !t.startsWith("-") && t.length > 0);
    return p ? shellFilename(p) : null;
  }

  /**
   * Interprets a shell command string into a human-readable action sentence.
   * Used in operator mode so non-technical users understand what the agent is doing.
   */
  function describeBashCommand(raw: string): string {
    // Strip leading shell modifiers (cd /path && CMD, env VAR=x CMD, etc.)
    const cleaned = raw
      .replace(/^.*&&\s*/, "")   // strip everything before last &&
      .replace(/^\s*\S+=\S+\s+/, "") // strip leading VAR=value
      .trim();

    const tokens = cleaned.split(/\s+/).filter(Boolean);
    if (tokens.length === 0) return "Running a command";

    const base = tokens[0].split("/").pop() ?? tokens[0]; // handle /usr/bin/git → git
    const rest = tokens.slice(1);

    switch (base) {
      // ── File reading ──────────────────────────────────────────────────
      case "cat":
      case "head":
      case "tail":
      case "less":
      case "more": {
        const f = firstPath(rest);
        return f ? `Reading ${f}` : "Reading a file";
      }
      // ── File listing / navigation ─────────────────────────────────────
      case "ls":
      case "dir":
      case "tree": {
        const f = firstPath(rest);
        return f ? `Exploring ${f}` : "Exploring a directory";
      }
      case "pwd":
        return "Checking current directory";
      case "cd":
        return rest[0] ? `Navigating to ${shellFilename(rest[0])}` : "Navigating";
      // ── File operations ───────────────────────────────────────────────
      case "cp": {
        const f = firstPath(rest.filter((t) => !t.startsWith("-")));
        return f ? `Copying ${f}` : "Copying files";
      }
      case "mv": {
        const f = firstPath(rest.filter((t) => !t.startsWith("-")));
        return f ? `Moving ${f}` : "Moving files";
      }
      case "rm":
      case "rmdir":
      case "unlink": {
        const f = firstPath(rest.filter((t) => !t.startsWith("-")));
        return f ? `Deleting ${f}` : "Deleting files";
      }
      case "mkdir":
      case "mkdirp": {
        const f = firstPath(rest.filter((t) => !t.startsWith("-")));
        return f ? `Creating directory ${f}` : "Creating directory";
      }
      case "touch": {
        const f = firstPath(rest);
        return f ? `Creating file ${f}` : "Creating a file";
      }
      case "chmod":
      case "chown":
        return "Changing file permissions";
      // ── Text search ───────────────────────────────────────────────────
      case "grep":
      case "rg":
      case "ag":
      case "ack": {
        const pattern = rest.find((t) => !t.startsWith("-"));
        return pattern ? `Searching for "${pattern.replace(/^['"]|['"]$/g, "").slice(0, 40)}"` : "Searching in files";
      }
      case "find": {
        const name = rest.findIndex((t) => t === "-name") >= 0
          ? rest[rest.findIndex((t) => t === "-name") + 1]
          : null;
        return name ? `Finding ${name.replace(/^['"]|['"]$/g, "")} files` : "Finding files";
      }
      // ── Text output ───────────────────────────────────────────────────
      case "echo":
      case "printf":
      case "print":
        return "Writing text output";
      case "tee":
      case "write": {
        const f = firstPath(rest.filter((t) => !t.startsWith("-")));
        return f ? `Writing to ${f}` : "Writing to file";
      }
      // ── Git ───────────────────────────────────────────────────────────
      case "git": {
        const sub = rest.find((t) => !t.startsWith("-")) ?? "";
        const gitActions: Record<string, string> = {
          diff: "Checking code changes",
          "diff-index": "Checking code changes",
          show: "Viewing commit details",
          log: "Checking commit history",
          status: "Checking repository status",
          add: "Staging changes",
          commit: "Saving a commit",
          push: "Publishing changes",
          pull: "Downloading latest changes",
          fetch: "Fetching remote changes",
          clone: "Cloning repository",
          checkout: "Switching branch",
          switch: "Switching branch",
          branch: "Managing branches",
          merge: "Merging changes",
          rebase: "Rebasing commits",
          reset: "Resetting changes",
          stash: "Stashing changes",
          tag: "Managing tags",
          remote: "Managing remotes",
          blame: "Checking file history",
          grep: "Searching in code history",
          apply: "Applying patch",
          cherry: "Cherry-picking commit",
          "cherry-pick": "Cherry-picking commit",
          format: "Formatting patch",
          bisect: "Bisecting history",
        };
        return gitActions[sub] ?? "Git operation";
      }
      // ── Package managers ──────────────────────────────────────────────
      case "npm":
      case "yarn":
      case "pnpm":
      case "bun": {
        const sub = rest[0] ?? "";
        const pkgActions: Record<string, string> = {
          install: "Installing dependencies",
          i: "Installing dependencies",
          ci: "Installing dependencies",
          add: "Adding a dependency",
          remove: "Removing a dependency",
          run: `Running script ${rest[1] ?? ""}`.trim(),
          build: "Building the project",
          test: "Running tests",
          start: "Starting the application",
          dev: "Starting development server",
          lint: "Linting code",
          format: "Formatting code",
          publish: "Publishing package",
          update: "Updating dependencies",
          upgrade: "Upgrading dependencies",
          outdated: "Checking for updates",
          audit: "Auditing dependencies",
        };
        return pkgActions[sub] ?? `Running ${base} command`;
      }
      // ── Rust / Cargo ──────────────────────────────────────────────────
      case "cargo": {
        const sub = rest[0] ?? "";
        const cargoActions: Record<string, string> = {
          build: "Building Rust project",
          check: "Checking Rust code",
          test: "Running Rust tests",
          run: "Running Rust program",
          fmt: "Formatting Rust code",
          clippy: "Linting Rust code",
          doc: "Generating documentation",
          clean: "Cleaning build artifacts",
          add: "Adding Rust dependency",
          update: "Updating Rust dependencies",
          publish: "Publishing crate",
          bench: "Running benchmarks",
        };
        return cargoActions[sub] ?? "Cargo command";
      }
      // ── Python ────────────────────────────────────────────────────────
      case "python":
      case "python3": {
        const script = firstPath(rest.filter((t) => !t.startsWith("-")));
        return script ? `Running ${script}` : "Running Python script";
      }
      case "pip":
      case "pip3": {
        const sub = rest[0] ?? "";
        return sub === "install" ? "Installing Python packages"
          : sub === "uninstall" ? "Removing Python packages"
          : "Managing Python packages";
      }
      // ── Node.js ───────────────────────────────────────────────────────
      case "node":
      case "ts-node":
      case "tsx": {
        const script = firstPath(rest.filter((t) => !t.startsWith("-")));
        return script ? `Running ${script}` : "Running Node.js script";
      }
      // ── Network ───────────────────────────────────────────────────────
      case "curl":
      case "wget":
      case "http":
      case "httpie": {
        const url = rest.find((t) => t.startsWith("http") || t.includes("."));
        if (url) {
          try { return `Fetching ${new URL(url).hostname}`; } catch { /* fallthrough */ }
        }
        return "Fetching from web";
      }
      case "ping":
      case "nc":
      case "telnet":
        return "Testing network connection";
      case "ssh":
        return "Connecting via SSH";
      case "scp":
        return "Transferring files via SSH";
      case "rsync":
        return "Synchronizing files";
      // ── System utilities ─────────────────────────────────────────────
      case "which":
      case "type":
        return "Locating a program";
      case "wc":
        return "Counting lines or words";
      case "sort":
        return "Sorting data";
      case "uniq":
        return "Deduplicating data";
      case "cut":
      case "awk":
      case "sed":
        return "Processing text";
      case "jq":
        return "Processing JSON data";
      case "xargs":
        return "Running batch commands";
      case "env":
      case "printenv":
        return "Checking environment variables";
      case "export":
        return "Setting environment variable";
      case "source":
      case ".":
        return "Loading script";
      case "make":
      case "cmake":
      case "ninja": {
        const target = firstPath(rest.filter((t) => !t.startsWith("-")));
        return target ? `Building ${target}` : "Building project";
      }
      case "docker": {
        const sub = rest[0] ?? "";
        const dockerActions: Record<string, string> = {
          build: "Building Docker image",
          run: "Starting container",
          stop: "Stopping container",
          pull: "Downloading Docker image",
          push: "Pushing Docker image",
          ps: "Listing containers",
          images: "Listing images",
          exec: "Running command in container",
          logs: "Reading container logs",
          compose: `Docker Compose: ${rest[1] ?? ""}`.trim(),
        };
        return dockerActions[sub] ?? "Docker operation";
      }
      case "kubectl": {
        const sub = rest[0] ?? "";
        return sub ? `Kubernetes: ${sub}` : "Kubernetes operation";
      }
      case "aws":
      case "gcloud":
      case "az":
        return "Cloud operation";
      case "test":
      case "[[":
        return "Checking condition";
      case "sleep":
        return "Waiting";
      case "kill":
      case "pkill":
        return "Stopping process";
      case "ps":
        return "Listing processes";
      case "df":
      case "du":
      case "free":
        return "Checking system resources";
      case "date":
        return "Checking date and time";
      default:
        // Last resort: if it looks like a path/script execution
        if (base.endsWith(".sh") || base.endsWith(".bash")) return `Running script ${base}`;
        if (base.endsWith(".py")) return `Running ${base}`;
        return "Running a command";
    }
  }

  /**
   * Derives a rich contextual description for a tool call from its input JSON.
   * Parses input_preview (args_json) to extract path, command, URL, etc.
   * Falls back to the i18n label if no meaningful detail can be extracted.
   */
  function describeToolCall(toolName: string, inputPreview: string | undefined): string {
    const fallback = $t(getToolLabelKey(toolName));
    if (!inputPreview) return fallback;
    let input: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(inputPreview);
      if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return fallback;
      input = parsed as Record<string, unknown>;
    } catch {
      return fallback;
    }
    const str = (key: string, max = 50): string | null => {
      const v = input[key];
      return typeof v === "string" && v.length > 0 ? (v.length > max ? "…" + v.slice(-(max - 1)) : v) : null;
    };
    switch (toolName) {
      case "file_read": {
        const p = str("path", 55);
        return p ? `Reading ${p}` : fallback;
      }
      case "file_write": {
        const p = str("path", 55);
        return p ? `Writing to ${p}` : fallback;
      }
      case "file_edit": {
        const p = str("path", 55);
        return p ? `Editing ${p}` : fallback;
      }
      case "file_list": {
        const p = str("path", 55);
        return p ? `Exploring ${p}` : fallback;
      }
      case "file_glob": {
        const pattern = str("pattern", 40);
        return pattern ? `Searching for ${pattern}` : fallback;
      }
      case "file_grep": {
        const pattern = str("pattern", 40);
        return pattern ? `Searching "${pattern}"` : fallback;
      }
      case "bash_executor": {
        const cmd = str("command", 200);
        if (!cmd) return fallback;
        // In builder mode the raw command is shown in the expand section;
        // here we always return a human-readable description.
        return describeBashCommand(cmd);
      }
      case "python_executor": {
        return fallback;
      }
      case "http_fetch": {
        const url = str("url", 100);
        const method = str("method") ?? "GET";
        if (url) {
          try {
            const hostname = new URL(url).hostname;
            return method.toUpperCase() === "GET" ? `Fetching ${hostname}` : `${method.toUpperCase()} → ${hostname}`;
          } catch {
            return `${method.toUpperCase()} ${url.slice(0, 40)}`;
          }
        }
        return fallback;
      }
      case "memory_search": {
        const query = str("query", 45);
        return query ? `Searching memory: "${query}"` : fallback;
      }
      default: {
        if (toolName.startsWith("mcp:")) {
          const parts = toolName.slice(4).split("/");
          return parts.length >= 2 ? `${parts[0]}: ${parts[1]}` : toolName.slice(4);
        }
        return fallback;
      }
    }
  }

  /** Human name used in builder mode / technical context */
  const TOOL_HUMAN_NAMES: Record<string, string> = {
    file_read: "File Read",
    file_write: "File Write",
    file_edit: "File Edit",
    file_list: "File List",
    file_glob: "File Search",
    file_grep: "Code Search",
    bash_executor: "Bash",
    python_executor: "Python",
    http_fetch: "HTTP Fetch",
    memory_search: "Memory Search",
    memory_store: "Memory Store",
  };

  function humanizeToolName(name: string): string {
    if (TOOL_HUMAN_NAMES[name]) return TOOL_HUMAN_NAMES[name];
    if (name.startsWith("mcp:")) {
      const parts = name.slice(4).split("/");
      return parts.length >= 2 ? `${parts[0]}: ${parts[1]}` : name.slice(4);
    }
    return name.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  }

  function toolInputLabel(toolName: string): string {
    if (toolName === "bash_executor" || toolName === "python_executor") return "Command";
    if (toolName === "http_fetch") return "Request";
    if (toolName.startsWith("file_")) return "Path / Args";
    return "Input";
  }

  function modelBadgeClass(hint: string): string {
    const normalized = hint.toLowerCase();
    for (const [key, value] of Object.entries(MODEL_BADGE_COLORS)) {
      if (normalized.includes(key)) return value;
    }
    return DEFAULT_MODEL_BADGE;
  }

  function modelBadgeLabel(hint: string | undefined): string {
    return hint ?? "default";
  }

  async function fetchTimeline() {
    if (!taskId) return;
    loading = events.length === 0;
    error = null;
    try {
      const result: TimelineEvent[] = await invoke("get_task_timeline", { taskId });
      events = result;
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function toggleToolExpand(index: number) {
    const next = new Set(expandedTools);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    expandedTools = next;
  }

  function toggleObservationExpand(index: number) {
    const next = new Set(expandedObservations);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    expandedObservations = next;
  }

  function formatDurationMs(ms: number | undefined): string {
    if (ms === undefined || ms === null) return "";
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  function formatCost(usd: number | undefined): string {
    if (usd === undefined || usd === null) return "";
    return usd < 0.01 ? `$${usd.toFixed(4)}` : `$${usd.toFixed(2)}`;
  }

  function truncateValue(value: string): string {
    if (value.length <= OBSERVATION_PREVIEW_LENGTH) return value;
    return value.slice(0, OBSERVATION_PREVIEW_LENGTH) + "...";
  }

  function executionModeLabel(mode: string, score: number | undefined): string {
    const style = EXECUTION_MODE_STYLES[mode];
    if (!style) return mode;
    const isBuilder = $uiMode === "builder";
    const label = isBuilder ? style.builder : style.operator;
    if (isBuilder && score !== undefined) return `${label} (${score.toFixed(2)})`;
    return label;
  }

  function executionModeColor(mode: string): string {
    return EXECUTION_MODE_STYLES[mode]?.color ?? "bg-muted text-muted-foreground";
  }

  // Deduplicate consecutive task_transition events with identical status
  const deduplicatedEvents = $derived(
    events.filter((event, index) => {
      if (event.type !== "task_transition") return true;
      const prev = events[index - 1];
      return !(prev && prev.type === "task_transition" && prev.status === event.status);
    })
  );

  let hasCacheHit = $derived(deduplicatedEvents.some((e) => e.type === "plan_cache_hit"));
  let cacheHitEvent = $derived(deduplicatedEvents.find((e) => e.type === "plan_cache_hit"));

  function eventSummary(event: TimelineEvent): string {
    switch (event.type) {
      case "task_transition": return $t(STATUS_I18N[event.status] ?? "dashboard.status_submitted");
      case "step_started": return event.tool ? `Step ${event.step_id} — ${event.tool}` : `Step ${event.step_id}`;
      case "step_completed": return `Step ${event.step_id} ${event.success ? "done" : "failed"} ${formatDurationMs(event.duration_ms)}`;
      case "llm_call": {
        const tokens = event.prompt_tokens !== undefined && event.completion_tokens !== undefined
          ? `${event.prompt_tokens}+${event.completion_tokens}t` : "";
        return [event.model, tokens, formatCost(event.cost_usd)].filter(Boolean).join(" · ");
      }
      case "tool_call": {
        const name = humanizeToolName(event.tool_name);
        const duration = formatDurationMs(event.duration_ms);
        return duration ? `${name} · ${duration}` : name;
      }
      case "hitl_suspended": return `Waiting — ${event.prompt.slice(0, 60)}`;
      case "hitl_resolved": return event.approved ? $t('common.approved') : $t('common.rejected');
      case "task_completed": return formatDurationMs(event.duration_ms) ? `Done in ${formatDurationMs(event.duration_ms)}` : "Done";
      case "step_observation": return $uiMode === "builder" ? $t('tasks.step_observation_builder') : $t('tasks.step_observation_operator');
      case "plan_cache_hit": return $uiMode === "builder" ? $t('tasks.cache_hit_builder') : $t('tasks.cache_hit_operator');
    }
  }

  $effect(() => {
    void fetchTimeline();
    let timer: ReturnType<typeof setInterval> | null = null;
    if (isRunning) timer = setInterval(() => void fetchTimeline(), POLL_INTERVAL_MS);
    return () => { if (timer !== null) clearInterval(timer); };
  });
</script>

<div class="space-y-0.5">
  {#if loading}
    <div class="flex items-center gap-2 py-3">
      <RefreshCw size={13} class="animate-spin text-muted-foreground" />
      <span class="text-xs text-muted-foreground">{$t('tasks.loading_timeline')}</span>
    </div>
  {:else if error}
    <p class="text-xs text-destructive py-2">{error}</p>
  {:else if deduplicatedEvents.length === 0}
    <p class="text-xs text-muted-foreground py-2">{$t('tasks.no_timeline')}</p>
  {:else}
    <!-- Cache hit banner -->
    {#if hasCacheHit && cacheHitEvent && cacheHitEvent.type === "plan_cache_hit"}
      <div
        class="mb-3 flex items-center gap-2 rounded-lg bg-gradient-to-r from-primary/5 to-transparent py-2 px-3"
        data-testid="timeline-cache-hit"
        title={$t('tasks.cache_hit_tooltip')}
        in:fly={{ y: 3, duration: 100 }}
      >
        <Zap size={13} class="shrink-0 text-primary" />
        <span class="text-xs text-foreground/70">
          {$uiMode === "builder" ? $t('tasks.cache_hit_builder') : $t('tasks.cache_hit_operator')}
        </span>
      </div>
    {/if}

    <div class="relative ml-2 border-l border-border/50 pl-4 space-y-3" data-testid="task-timeline">
      {#each deduplicatedEvents as event, index (index)}
        <div
          class="relative"
          data-testid="timeline-event"
          data-event-type={event.type}
          in:fly={{ y: 3, duration: 100, delay: index * 15 }}
        >
          <!-- Dot -->
          <span class="absolute -left-[calc(1rem+4px)] top-[5px] h-2 w-2 rounded-full {EVENT_DOT_COLOR[event.type] ?? 'bg-muted-foreground'}"></span>

          <!-- Step observation entry -->
          {#if event.type === "step_observation"}
            <div class="flex items-start gap-2">
              <Brain size={13} class="mt-0.5 shrink-0 text-secondary" />
              <div class="min-w-0 flex-1">
                <div class="flex items-baseline gap-2">
                  <span class="text-xs text-foreground/70">{eventSummary(event)}</span>
                  <Badge variant="outline" class="text-[9px] px-1 py-0">{event.memory_key}</Badge>
                  <span class="ml-auto shrink-0 text-[11px] text-muted-foreground/35">{formatRelativeTime(event.timestamp)}</span>
                </div>
                <button
                  class="mt-1 flex items-center gap-1 text-[11px] text-muted-foreground/50 hover:text-foreground transition-colors"
                  data-testid="timeline-step-observation"
                  onclick={() => toggleObservationExpand(index)}
                  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleObservationExpand(index); } }}
                >
                  {#if expandedObservations.has(index)}
                    <ChevronDown size={11} />
                  {:else}
                    <ChevronRight size={11} />
                  {/if}
                  <span class="text-muted-foreground">{truncateValue(event.memory_value)}</span>
                </button>
                {#if expandedObservations.has(index)}
                  <div
                    class="mt-1.5 rounded-md bg-muted/40 px-3 py-2"
                    data-testid="step-observation-detail"
                    transition:slide={{ duration: 150 }}
                  >
                    <pre class="whitespace-pre-wrap text-[11px] text-muted-foreground font-mono">{event.memory_value}</pre>
                  </div>
                {/if}
              </div>
            </div>

          <!-- Tool call — operator mode: icon + contextual description, no expand -->
          {:else if event.type === "tool_call" && $uiMode === "operator"}
            {@const ToolIcon = getToolIcon(event.tool_name)}
            <div class="flex items-center gap-2.5">
              <ToolIcon size={14} class="shrink-0 text-muted-foreground/60" />
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class="text-xs text-foreground/80 font-medium truncate">{describeToolCall(event.tool_name, event.input_preview)}</span>
                  {#if event.duration_ms !== undefined && event.duration_ms > 0}
                    <span class="shrink-0 text-[11px] text-muted-foreground/50">{formatDurationMs(event.duration_ms)}</span>
                  {/if}
                  <span class="ml-auto shrink-0 text-[11px] text-muted-foreground/35">{formatRelativeTime(event.timestamp)}</span>
                </div>
              </div>
            </div>

          <!-- Standard event entry -->
          {:else}
            <div class="flex items-baseline gap-2">
              <span class="text-xs text-foreground/70">{eventSummary(event)}</span>

              <!-- Task transition: status badge + execution mode badge -->
              {#if event.type === "task_transition"}
                <Badge variant="outline" class="text-[9px] px-1.5 py-0">{$t(STATUS_I18N[event.status] ?? "dashboard.status_submitted")}</Badge>
                {#if event.execution_mode}
                  <span
                    class="inline-flex items-center rounded-full px-1.5 py-0 text-[10px] font-medium {executionModeColor(event.execution_mode)}"
                    data-testid="timeline-execution-mode"
                  >
                    {executionModeLabel(event.execution_mode, event.complexity_score)}
                  </span>
                {/if}
              {/if}

              <!-- Step completed: model hint badge -->
              {#if event.type === "step_completed"}
                <span
                  class="inline-flex items-center rounded-full px-1.5 py-0 text-[10px] font-medium {event.model_hint ? modelBadgeClass(event.model_hint) : DEFAULT_MODEL_BADGE}"
                  data-testid="step-model-badge"
                >
                  {modelBadgeLabel(event.model_hint)}
                </span>
              {/if}

              <span class="ml-auto shrink-0 text-[11px] text-muted-foreground/35">{formatRelativeTime(event.timestamp)}</span>
            </div>

            <!-- Tool call expandable (builder only) -->
            {#if event.type === "tool_call" && $uiMode === "builder"}
              <button
                class="mt-0.5 flex items-center gap-1 text-[11px] text-muted-foreground/50 hover:text-foreground transition-colors"
                onclick={() => toggleToolExpand(index)}
              >
                {#if expandedTools.has(index)}
                  <ChevronDown size={11} />
                {:else}
                  <ChevronRight size={11} />
                {/if}
                {$t('tasks.show_details')}
              </button>
              {#if expandedTools.has(index)}
                <div
                  class="mt-1.5 rounded-md bg-muted/40 px-3 py-2.5 space-y-2.5"
                  data-testid="tool-call-detail"
                  transition:slide={{ duration: 150 }}
                >
                  <!-- Meta row -->
                  <div class="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-muted-foreground">
                    <span class="font-medium text-foreground/70">{humanizeToolName(event.tool_name)}</span>
                    {#if event.duration_ms !== undefined}
                      <span>{formatDurationMs(event.duration_ms)}</span>
                    {/if}
                    {#if event.exit_code !== undefined && event.exit_code !== null}
                      <span class={event.exit_code === 0 ? "text-success" : "text-destructive"}>
                        exit {event.exit_code}
                      </span>
                    {/if}
                    {#if event.truncated}
                      <span class="text-warning">{$t('tasks.output_truncated')}</span>
                    {/if}
                  </div>

                  <!-- Input preview -->
                  {#if event.input_preview}
                    <div>
                      <p class="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/50">
                        {toolInputLabel(event.tool_name)}
                      </p>
                      <pre class="whitespace-pre-wrap rounded bg-muted/60 px-2.5 py-1.5 font-mono text-[11px] text-muted-foreground leading-relaxed overflow-x-auto">{event.input_preview}</pre>
                    </div>
                  {/if}

                  <!-- Output preview -->
                  {#if event.output_preview}
                    <div>
                      <p class="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/50">
                        {$t('tasks.tool_output_label')}
                      </p>
                      <pre class="whitespace-pre-wrap rounded bg-muted/60 px-2.5 py-1.5 font-mono text-[11px] text-muted-foreground leading-relaxed overflow-x-auto max-h-48 overflow-y-auto">{event.output_preview}</pre>
                    </div>
                  {/if}

                  <!-- Fallback -->
                  {#if !event.input_preview && !event.output_preview}
                    <p class="text-[11px] text-muted-foreground/50 italic">{$t('tasks.no_tool_details')}</p>
                  {/if}
                </div>
              {/if}
            {/if}

            {#if event.type === "llm_call" && event.latency_ms !== undefined}
              <span class="text-[11px] text-muted-foreground/35">{$t('tasks.latency')}: {formatDurationMs(event.latency_ms)}</span>
            {/if}
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
