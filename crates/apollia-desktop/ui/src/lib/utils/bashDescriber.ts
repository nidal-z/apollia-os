/**
 * Translates shell commands and tool calls into human-readable action
 * sentences for the operator skin. Extracted from `TaskTimeline.svelte`
 * (ADR-088, Lot 3) so it's reusable by `TraceEventCard` and unit-testable
 * in isolation.
 *
 * The functions here are pure — no Svelte / DOM / store dependencies.
 * Operators see "Reading file.txt" instead of `cat /path/to/file.txt`,
 * "Pushing changes" instead of `git push origin main`, etc.
 */

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

// Constant verbs that ignore arguments — kept as a lookup map to keep
// `describeBashCommand` flat (S1479 / S3776).
const CONSTANT_VERBS: Record<string, string> = {
  pwd: "Checking current directory",
  chmod: "Changing file permissions",
  chown: "Changing file permissions",
  echo: "Writing text output",
  printf: "Writing text output",
  print: "Writing text output",
  ping: "Testing network connection",
  nc: "Testing network connection",
  telnet: "Testing network connection",
  ssh: "Connecting via SSH",
  scp: "Transferring files via SSH",
  rsync: "Synchronizing files",
  which: "Locating a program",
  type: "Locating a program",
  wc: "Counting lines or words",
  sort: "Sorting data",
  uniq: "Deduplicating data",
  cut: "Processing text",
  awk: "Processing text",
  sed: "Processing text",
  jq: "Processing JSON data",
  xargs: "Running batch commands",
  env: "Checking environment variables",
  printenv: "Checking environment variables",
  export: "Setting environment variable",
  source: "Loading script",
  ".": "Loading script",
  aws: "Cloud operation",
  gcloud: "Cloud operation",
  az: "Cloud operation",
  test: "Checking condition",
  "[[": "Checking condition",
  sleep: "Waiting",
  kill: "Stopping process",
  pkill: "Stopping process",
  ps: "Listing processes",
  df: "Checking system resources",
  du: "Checking system resources",
  free: "Checking system resources",
  date: "Checking date and time",
};

const GIT_ACTIONS: Record<string, string> = {
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

const CARGO_ACTIONS: Record<string, string> = {
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

type Handler = (rest: string[], base: string) => string;

function describeReadFamily(rest: string[]): string {
  const f = firstPath(rest);
  return f ? `Reading ${f}` : "Reading a file";
}

function describeExploreFamily(rest: string[]): string {
  const f = firstPath(rest);
  return f ? `Exploring ${f}` : "Exploring a directory";
}

function describeCd(rest: string[]): string {
  return rest[0] ? `Navigating to ${shellFilename(rest[0])}` : "Navigating";
}

function describeNonFlagFirstPath(
  rest: string[],
  withPathLabel: (path: string) => string,
  fallback: string,
): string {
  const f = firstPath(rest.filter((t) => !t.startsWith("-")));
  return f ? withPathLabel(f) : fallback;
}

function describeTouch(rest: string[]): string {
  const f = firstPath(rest);
  return f ? `Creating file ${f}` : "Creating a file";
}

function describeGrep(rest: string[]): string {
  const pattern = rest.find((t) => !t.startsWith("-"));
  return pattern
    ? `Searching for "${pattern.replaceAll(/^['"]|['"]$/g, "").slice(0, 40)}"`
    : "Searching in files";
}

function describeFind(rest: string[]): string {
  const idx = rest.indexOf("-name");
  const name = idx >= 0 ? rest[idx + 1] : null;
  return name
    ? `Finding ${name.replaceAll(/^['"]|['"]$/g, "")} files`
    : "Finding files";
}

function describeGit(rest: string[]): string {
  const sub = rest.find((t) => !t.startsWith("-")) ?? "";
  return GIT_ACTIONS[sub] ?? "Git operation";
}

function describeNodePkg(rest: string[], base: string): string {
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

function describeCargo(rest: string[]): string {
  const sub = rest[0] ?? "";
  return CARGO_ACTIONS[sub] ?? "Cargo command";
}

function describePython(rest: string[]): string {
  const script = firstPath(rest.filter((t) => !t.startsWith("-")));
  return script ? `Running ${script}` : "Running Python script";
}

function describePip(rest: string[]): string {
  const sub = rest[0] ?? "";
  if (sub === "install") return "Installing Python packages";
  if (sub === "uninstall") return "Removing Python packages";
  return "Managing Python packages";
}

function describeNode(rest: string[]): string {
  const script = firstPath(rest.filter((t) => !t.startsWith("-")));
  return script ? `Running ${script}` : "Running Node.js script";
}

function describeHttp(rest: string[]): string {
  const url = rest.find((t) => t.startsWith("http") || t.includes("."));
  if (url) {
    try {
      return `Fetching ${new URL(url).hostname}`;
    } catch {
      /* fallthrough */
    }
  }
  return "Fetching from web";
}

function describeMake(rest: string[]): string {
  const target = firstPath(rest.filter((t) => !t.startsWith("-")));
  return target ? `Building ${target}` : "Building project";
}

function describeDocker(rest: string[]): string {
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

function describeKubectl(rest: string[]): string {
  const sub = rest[0] ?? "";
  return sub ? `Kubernetes: ${sub}` : "Kubernetes operation";
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
  cp: (rest) => describeNonFlagFirstPath(rest, (f) => `Copying ${f}`, "Copying files"),
  mv: (rest) => describeNonFlagFirstPath(rest, (f) => `Moving ${f}`, "Moving files"),
  rm: (rest) => describeNonFlagFirstPath(rest, (f) => `Deleting ${f}`, "Deleting files"),
  rmdir: (rest) => describeNonFlagFirstPath(rest, (f) => `Deleting ${f}`, "Deleting files"),
  unlink: (rest) => describeNonFlagFirstPath(rest, (f) => `Deleting ${f}`, "Deleting files"),
  mkdir: (rest) => describeNonFlagFirstPath(rest, (f) => `Creating directory ${f}`, "Creating directory"),
  mkdirp: (rest) => describeNonFlagFirstPath(rest, (f) => `Creating directory ${f}`, "Creating directory"),
  touch: describeTouch,
  grep: describeGrep,
  rg: describeGrep,
  ag: describeGrep,
  ack: describeGrep,
  find: describeFind,
  tee: (rest) => describeNonFlagFirstPath(rest, (f) => `Writing to ${f}`, "Writing to file"),
  write: (rest) => describeNonFlagFirstPath(rest, (f) => `Writing to ${f}`, "Writing to file"),
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

function describeByExtension(base: string): string {
  if (base.endsWith(".sh") || base.endsWith(".bash")) return `Running script ${base}`;
  if (base.endsWith(".py")) return `Running ${base}`;
  return "Running a command";
}

/**
 * Interprets a shell command string into a human-readable action sentence.
 *
 * Example:
 *   describeBashCommand("cat /etc/hosts")           → "Reading hosts"
 *   describeBashCommand("git push origin main")     → "Publishing changes"
 *   describeBashCommand("npm run build")            → "Running script build"
 *
 * Falls back to "Running a command" for unknown invocations rather than
 * exposing raw shell syntax to non-technical operators.
 */
export function describeBashCommand(raw: string): string {
  // Strip leading shell modifiers (cd /path && CMD, env VAR=x CMD, etc.)
  const cleaned = raw
    .replace(/^.*&&\s*/, "")
    .replace(/^\s*\S+=\S+\s+/, "")
    .trim();

  const tokens = cleaned.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return "Running a command";

  const base = tokens[0].split("/").pop() ?? tokens[0];
  const rest = tokens.slice(1);

  const constant = CONSTANT_VERBS[base];
  if (constant !== undefined) return constant;

  const handler = HANDLERS[base];
  if (handler !== undefined) return handler(rest, base);

  return describeByExtension(base);
}

/**
 * Derives a rich operator-friendly description from a tool call's input JSON.
 *
 * Parses common shapes (path, command, url, query, content snippet) into
 * one short sentence. Returns the i18n fallback label if the input is
 * empty or unparseable.
 *
 * @param toolName - the tool name (`file_read`, `web_search`, `bash_executor`…)
 * @param inputJson - the args JSON string (from `tool_call_started.args_json`)
 * @param fallback - the i18n label to return when no detail can be extracted
 */
export function describeToolCall(
  toolName: string,
  inputJson: string | null | undefined,
  fallback: string,
): string {
  if (!inputJson) return fallback;
  let input: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(inputJson);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed))
      return fallback;
    input = parsed as Record<string, unknown>;
  } catch {
    return fallback;
  }
  const str = (key: string, max = 50): string | null => {
    const v = input[key];
    if (typeof v !== "string" || v.length === 0) return null;
    return v.length > max ? "…" + v.slice(-(max - 1)) : v;
  };

  type ToolCallDescriber = () => string;
  const describeUrl = (max: number): string => {
    const url = str("url", max);
    if (!url) return fallback;
    try {
      return `Fetching ${new URL(url).hostname}`;
    } catch {
      return `Fetching ${url}`;
    }
  };

  const describers: Record<string, ToolCallDescriber> = {
    file_read: () => {
      const p = str("path", 55);
      return p ? `Reading ${p}` : fallback;
    },
    file_write: () => {
      const p = str("path", 55);
      return p ? `Writing to ${p}` : fallback;
    },
    file_edit: () => {
      const p = str("path", 55);
      return p ? `Editing ${p}` : fallback;
    },
    file_list: () => {
      const p = str("dir", 55) ?? str("pattern", 55) ?? str("path", 55);
      return p ? `Listing ${p}` : fallback;
    },
    file_glob: () => {
      const p = str("dir", 55) ?? str("pattern", 55) ?? str("path", 55);
      return p ? `Listing ${p}` : fallback;
    },
    file_grep: () => {
      const p = str("pattern", 40);
      return p ? `Searching for "${p}"` : fallback;
    },
    bash_executor: () => {
      const cmd = str("command", 200);
      return cmd ? describeBashCommand(cmd) : fallback;
    },
    python_executor: () => {
      const code = str("code", 60);
      return code ? `Running Python: ${code}` : fallback;
    },
    http_fetch: () => describeUrl(200),
    web_read: () => describeUrl(200),
    web_search: () => {
      const q = str("query", 60);
      return q ? `Searching the web for "${q}"` : fallback;
    },
    memory_search: () => {
      const q = str("query", 60);
      return q ? `Searching memory for "${q}"` : fallback;
    },
    ask_user: () => "Asking the user a question",
  };

  const describer = describers[toolName];
  if (describer) return describer();

  if (toolName.startsWith("a2a:")) return `Delegating to ${toolName.slice(4)}`;
  if (toolName.startsWith("mcp:")) return `Calling external tool ${toolName.slice(4)}`;
  return fallback;
}
