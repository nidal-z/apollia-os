/**
 * Maps shell commands and tool calls to catalogue keys under
 * `trace.describe.*`, so the operator skin of the execution trace renders
 * "Reading file.txt" / "Lecture de file.txt" through `$t` instead of a
 * hardcoded English sentence. Extracted from `TaskTimeline.svelte`, consumed
 * by `TraceEventCard`.
 *
 * The functions here are pure - no Svelte / DOM / store dependencies. They
 * return an [`OperatorLabel`] (key plus interpolation values); the component
 * passes it to `$t(label.key, { values: label.values })`.
 */

/** A catalogue key plus the values its message interpolates. */
export interface OperatorLabel {
  key: string;
  values?: Record<string, string>;
}

const BASE = "trace.describe";

function label(suffix: string, values?: Record<string, string>): OperatorLabel {
  return values ? { key: `${BASE}.${suffix}`, values } : { key: `${BASE}.${suffix}` };
}

/** Extracts the last path segment (filename) from a shell argument. */
export function shellFilename(arg: string): string {
  return (
    arg
      .replaceAll(/^['"]|['"]$/g, "")
      .split(/[/\\]/)
      .findLast((part) => part.length > 0) ?? arg
  );
}

/** Extracts the first non-flag argument from a token list. */
export function firstPath(tokens: string[]): string | null {
  const p = tokens.find((t) => !t.startsWith("-") && t.length > 0);
  return p ? shellFilename(p) : null;
}

// Constant verbs that ignore arguments - kept as a lookup map to keep
// `describeBashCommand` flat (S1479 / S3776).
const CONSTANT_VERBS: Record<string, string> = {
  pwd: "checking_current_directory",
  chmod: "changing_file_permissions",
  chown: "changing_file_permissions",
  echo: "writing_text_output",
  printf: "writing_text_output",
  print: "writing_text_output",
  ping: "testing_network_connection",
  nc: "testing_network_connection",
  telnet: "testing_network_connection",
  ssh: "connecting_ssh",
  scp: "transferring_ssh",
  rsync: "synchronizing_files",
  which: "locating_program",
  type: "locating_program",
  wc: "counting_lines",
  sort: "sorting_data",
  uniq: "deduplicating_data",
  cut: "processing_text",
  awk: "processing_text",
  sed: "processing_text",
  jq: "processing_json",
  xargs: "running_batch_commands",
  env: "checking_env",
  printenv: "checking_env",
  export: "setting_env",
  source: "loading_script",
  ".": "loading_script",
  aws: "cloud_operation",
  gcloud: "cloud_operation",
  az: "cloud_operation",
  test: "checking_condition",
  "[[": "checking_condition",
  sleep: "waiting",
  kill: "stopping_process",
  pkill: "stopping_process",
  ps: "listing_processes",
  df: "checking_resources",
  du: "checking_resources",
  free: "checking_resources",
  date: "checking_datetime",
};

const GIT_ACTIONS: Record<string, string> = {
  diff: "checking_code_changes",
  "diff-index": "checking_code_changes",
  show: "viewing_commit_details",
  log: "checking_commit_history",
  status: "checking_repo_status",
  add: "staging_changes",
  commit: "saving_commit",
  push: "publishing_changes",
  pull: "downloading_latest_changes",
  fetch: "fetching_remote_changes",
  clone: "cloning_repository",
  checkout: "switching_branch",
  switch: "switching_branch",
  branch: "managing_branches",
  merge: "merging_changes",
  rebase: "rebasing_commits",
  reset: "resetting_changes",
  stash: "stashing_changes",
  tag: "managing_tags",
  remote: "managing_remotes",
  blame: "checking_file_history",
  grep: "searching_code_history",
  apply: "applying_patch",
  cherry: "cherry_picking",
  "cherry-pick": "cherry_picking",
  format: "formatting_patch",
  bisect: "bisecting_history",
};

const CARGO_ACTIONS: Record<string, string> = {
  build: "building_rust_project",
  check: "checking_rust_code",
  test: "running_rust_tests",
  run: "running_rust_program",
  fmt: "formatting_rust_code",
  clippy: "linting_rust_code",
  doc: "generating_documentation",
  clean: "cleaning_build_artifacts",
  add: "adding_rust_dependency",
  update: "updating_rust_dependencies",
  publish: "publishing_crate",
  bench: "running_benchmarks",
};

type Handler = (rest: string[], base: string) => OperatorLabel;

function describeReadFamily(rest: string[]): OperatorLabel {
  const f = firstPath(rest);
  return f ? label("reading_named", { name: f }) : label("reading_file");
}

function describeExploreFamily(rest: string[]): OperatorLabel {
  const f = firstPath(rest);
  return f ? label("exploring_named", { name: f }) : label("exploring_directory");
}

function describeCd(rest: string[]): OperatorLabel {
  return rest[0]
    ? label("navigating_named", { name: shellFilename(rest[0]) })
    : label("navigating");
}

function describeNonFlagFirstPath(
  rest: string[],
  namedSuffix: string,
  fallbackSuffix: string,
): OperatorLabel {
  const f = firstPath(rest.filter((t) => !t.startsWith("-")));
  return f ? label(namedSuffix, { name: f }) : label(fallbackSuffix);
}

function describeTouch(rest: string[]): OperatorLabel {
  const f = firstPath(rest);
  return f ? label("creating_file_named", { name: f }) : label("creating_file");
}

function describeGrep(rest: string[]): OperatorLabel {
  const pattern = rest.find((t) => !t.startsWith("-"));
  return pattern
    ? label("searching_named", {
        pattern: pattern.replaceAll(/^['"]|['"]$/g, "").slice(0, 40),
      })
    : label("searching_in_files");
}

function describeFind(rest: string[]): OperatorLabel {
  const idx = rest.indexOf("-name");
  const name = idx >= 0 ? rest[idx + 1] : null;
  return name
    ? label("finding_named", { name: name.replaceAll(/^['"]|['"]$/g, "") })
    : label("finding_files");
}

function describeGit(rest: string[]): OperatorLabel {
  const sub = rest.find((t) => !t.startsWith("-")) ?? "";
  return label(GIT_ACTIONS[sub] ?? "git_operation");
}

function describeNodePkg(rest: string[], base: string): OperatorLabel {
  const sub = rest[0] ?? "";
  const pkgActions: Record<string, string> = {
    install: "installing_dependencies",
    i: "installing_dependencies",
    ci: "installing_dependencies",
    add: "adding_dependency",
    remove: "removing_dependency",
    build: "building_the_project",
    test: "running_tests",
    start: "starting_application",
    dev: "starting_dev_server",
    lint: "linting_code",
    format: "formatting_code",
    publish: "publishing_package",
    update: "updating_dependencies",
    upgrade: "upgrading_dependencies",
    outdated: "checking_for_updates",
    audit: "auditing_dependencies",
  };
  if (sub === "run") {
    return rest[1]
      ? label("running_script_named", { name: rest[1] })
      : label("running_command");
  }
  const suffix = pkgActions[sub];
  return suffix ? label(suffix) : label("running_named_command", { name: base });
}

function describeCargo(rest: string[]): OperatorLabel {
  const sub = rest[0] ?? "";
  return label(CARGO_ACTIONS[sub] ?? "cargo_command");
}

function describePython(rest: string[]): OperatorLabel {
  const script = firstPath(rest.filter((t) => !t.startsWith("-")));
  return script
    ? label("running_named", { name: script })
    : label("running_python_script");
}

function describePip(rest: string[]): OperatorLabel {
  const sub = rest[0] ?? "";
  if (sub === "install") return label("installing_python_packages");
  if (sub === "uninstall") return label("removing_python_packages");
  return label("managing_python_packages");
}

function describeNode(rest: string[]): OperatorLabel {
  const script = firstPath(rest.filter((t) => !t.startsWith("-")));
  return script
    ? label("running_named", { name: script })
    : label("running_node_script");
}

function describeHttp(rest: string[]): OperatorLabel {
  const url = rest.find((t) => t.startsWith("http") || t.includes("."));
  if (url) {
    try {
      return label("fetching_named", { name: new URL(url).hostname });
    } catch {
      /* fallthrough */
    }
  }
  return label("fetching_web");
}

function describeMake(rest: string[]): OperatorLabel {
  const target = firstPath(rest.filter((t) => !t.startsWith("-")));
  return target ? label("building_named", { name: target }) : label("building_project");
}

function describeDocker(rest: string[]): OperatorLabel {
  const sub = rest[0] ?? "";
  const dockerActions: Record<string, string> = {
    build: "building_docker_image",
    run: "starting_container",
    stop: "stopping_container",
    pull: "downloading_docker_image",
    push: "pushing_docker_image",
    ps: "listing_containers",
    images: "listing_images",
    exec: "running_in_container",
    logs: "reading_container_logs",
  };
  if (sub === "compose") {
    return rest[1]
      ? label("docker_compose_named", { name: rest[1] })
      : label("docker_operation");
  }
  const suffix = dockerActions[sub];
  return suffix ? label(suffix) : label("docker_operation");
}

function describeKubectl(rest: string[]): OperatorLabel {
  const sub = rest[0] ?? "";
  return sub ? label("kubernetes_named", { name: sub }) : label("kubernetes_operation");
}

const HANDLERS: Record<string, Handler> = {
  cat: describeReadFamily,
  head: describeReadFamily,
  tail: describeReadFamily,
  less: describeReadFamily,
  more: describeReadFamily,
  ls: describeExploreFamily,
  dir: describeExploreFamily,
  tree: describeExploreFamily,
  cd: describeCd,
  cp: (rest) => describeNonFlagFirstPath(rest, "copying_named", "copying_files"),
  mv: (rest) => describeNonFlagFirstPath(rest, "moving_named", "moving_files"),
  rm: (rest) => describeNonFlagFirstPath(rest, "deleting_named", "deleting_files"),
  rmdir: (rest) => describeNonFlagFirstPath(rest, "deleting_named", "deleting_files"),
  unlink: (rest) => describeNonFlagFirstPath(rest, "deleting_named", "deleting_files"),
  mkdir: (rest) =>
    describeNonFlagFirstPath(rest, "creating_directory_named", "creating_directory"),
  mkdirp: (rest) =>
    describeNonFlagFirstPath(rest, "creating_directory_named", "creating_directory"),
  touch: describeTouch,
  grep: describeGrep,
  rg: describeGrep,
  ag: describeGrep,
  ack: describeGrep,
  find: describeFind,
  tee: (rest) => describeNonFlagFirstPath(rest, "writing_named", "writing_to_file"),
  write: (rest) => describeNonFlagFirstPath(rest, "writing_named", "writing_to_file"),
  git: describeGit,
  npm: describeNodePkg,
  yarn: describeNodePkg,
  pnpm: describeNodePkg,
  bun: describeNodePkg,
  cargo: describeCargo,
  python: describePython,
  python3: describePython,
  pip: describePip,
  pip3: describePip,
  node: describeNode,
  "ts-node": describeNode,
  tsx: describeNode,
  curl: describeHttp,
  wget: describeHttp,
  http: describeHttp,
  httpie: describeHttp,
  make: describeMake,
  cmake: describeMake,
  ninja: describeMake,
  docker: describeDocker,
  kubectl: describeKubectl,
};

function describeByExtension(base: string): OperatorLabel {
  if (base.endsWith(".sh") || base.endsWith(".bash")) {
    return label("running_script_named", { name: base });
  }
  if (base.endsWith(".py")) return label("running_named", { name: base });
  return label("running_command");
}

/**
 * Interprets a shell command string into the catalogue key of a
 * human-readable action sentence.
 *
 * Example:
 *   describeBashCommand("cat /etc/hosts")       → reading_named {name: "hosts"}
 *   describeBashCommand("git push origin main") → publishing_changes
 *
 * Falls back to `trace.describe.running_command` for unknown invocations
 * rather than exposing raw shell syntax to non-technical operators.
 */
export function describeBashCommand(raw: string): OperatorLabel {
  // Strip leading shell modifiers (cd /path && CMD, env VAR=x CMD, etc.)
  const cleaned = raw
    .replace(/^.*&&\s*/, "")
    .replace(/^\s*\S+=\S+\s+/, "")
    .trim();

  const tokens = cleaned.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return label("running_command");

  const base = tokens[0].split("/").pop() ?? tokens[0];
  const rest = tokens.slice(1);

  const constant = CONSTANT_VERBS[base];
  if (constant !== undefined) return label(constant);

  const handler = HANDLERS[base];
  if (handler !== undefined) return handler(rest, base);

  return describeByExtension(base);
}

/**
 * Derives a rich operator-friendly description from a tool call's input JSON.
 *
 * Parses common shapes (path, command, url, query, content snippet) into the
 * key of one short sentence. Returns `null` when no detail can be extracted,
 * so the caller decides what to fall back on (typically the raw tool name).
 *
 * @param toolName - the tool name (`file_read`, `web_search`, `bash_executor`…)
 * @param inputJson - the args JSON string (from `tool_call_started.args_json`)
 */
export function describeToolCall(
  toolName: string,
  inputJson: string | null | undefined,
): OperatorLabel | null {
  const structural = describeToolName(toolName);
  if (!inputJson) return structural;
  let input: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(inputJson);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed))
      return structural;
    input = parsed as Record<string, unknown>;
  } catch {
    return structural;
  }
  const str = (key: string, max = 50): string | null => {
    const v = input[key];
    if (typeof v !== "string" || v.length === 0) return null;
    return v.length > max ? "…" + v.slice(-(max - 1)) : v;
  };

  type ToolCallDescriber = () => OperatorLabel | null;
  const describeUrl = (max: number): OperatorLabel | null => {
    const url = str("url", max);
    if (!url) return null;
    try {
      return label("fetching_named", { name: new URL(url).hostname });
    } catch {
      return label("fetching_named", { name: url });
    }
  };

  const describers: Record<string, ToolCallDescriber> = {
    file_read: () => {
      const p = str("path", 55);
      return p ? label("reading_named", { name: p }) : null;
    },
    file_write: () => {
      const p = str("path", 55);
      return p ? label("writing_named", { name: p }) : null;
    },
    file_edit: () => {
      const p = str("path", 55);
      return p ? label("editing_named", { name: p }) : null;
    },
    file_list: () => {
      const p = str("dir", 55) ?? str("pattern", 55) ?? str("path", 55);
      return p ? label("listing_named", { name: p }) : null;
    },
    file_glob: () => {
      const p = str("dir", 55) ?? str("pattern", 55) ?? str("path", 55);
      return p ? label("listing_named", { name: p }) : null;
    },
    file_grep: () => {
      const p = str("pattern", 40);
      return p ? label("searching_named", { pattern: p }) : null;
    },
    bash_executor: () => {
      const cmd = str("command", 200);
      return cmd ? describeBashCommand(cmd) : null;
    },
    python_executor: () => {
      const code = str("code", 60);
      return code ? label("running_python_code", { code }) : null;
    },
    http_fetch: () => describeUrl(200),
    web_read: () => describeUrl(200),
    web_search: () => {
      const q = str("query", 60);
      return q ? label("searching_web", { query: q }) : null;
    },
    memory_search: () => {
      const q = str("query", 60);
      return q ? label("searching_memory", { query: q }) : null;
    },
    ask_user: () => label("asking_user"),
  };

  const describer = describers[toolName];
  if (describer) return describer() ?? structural;
  return structural;
}

/**
 * Structural description derived from the tool name alone: A2A delegation and
 * MCP calls carry their target in the name. Returns `null` for anything else.
 */
function describeToolName(toolName: string): OperatorLabel | null {
  if (toolName.startsWith("a2a:")) {
    return label("delegating_named", { name: toolName.slice(4) });
  }
  if (toolName.startsWith("mcp:")) {
    return label("calling_external_tool", { name: toolName.slice(4) });
  }
  return null;
}
