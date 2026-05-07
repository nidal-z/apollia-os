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
      .replace(/^['"]|['"]$/g, "")
      .split(/[/\\]/)
      .filter(Boolean)
      .pop() ?? arg
  );
}

/** Extracts the first non-flag argument from a token list. */
export function firstPath(tokens: string[]): string | null {
  const p = tokens.find((t) => !t.startsWith("-") && t.length > 0);
  return p ? shellFilename(p) : null;
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

  switch (base) {
    case "cat":
    case "head":
    case "tail":
    case "less":
    case "more": {
      const f = firstPath(rest);
      return f ? `Reading ${f}` : "Reading a file";
    }
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
    case "grep":
    case "rg":
    case "ag":
    case "ack": {
      const pattern = rest.find((t) => !t.startsWith("-"));
      return pattern
        ? `Searching for "${pattern.replace(/^['"]|['"]$/g, "").slice(0, 40)}"`
        : "Searching in files";
    }
    case "find": {
      const idx = rest.findIndex((t) => t === "-name");
      const name = idx >= 0 ? rest[idx + 1] : null;
      return name
        ? `Finding ${name.replace(/^['"]|['"]$/g, "")} files`
        : "Finding files";
    }
    case "echo":
    case "printf":
    case "print":
      return "Writing text output";
    case "tee":
    case "write": {
      const f = firstPath(rest.filter((t) => !t.startsWith("-")));
      return f ? `Writing to ${f}` : "Writing to file";
    }
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
    case "python":
    case "python3": {
      const script = firstPath(rest.filter((t) => !t.startsWith("-")));
      return script ? `Running ${script}` : "Running Python script";
    }
    case "pip":
    case "pip3": {
      const sub = rest[0] ?? "";
      return sub === "install"
        ? "Installing Python packages"
        : sub === "uninstall"
          ? "Removing Python packages"
          : "Managing Python packages";
    }
    case "node":
    case "ts-node":
    case "tsx": {
      const script = firstPath(rest.filter((t) => !t.startsWith("-")));
      return script ? `Running ${script}` : "Running Node.js script";
    }
    case "curl":
    case "wget":
    case "http":
    case "httpie": {
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
      if (base.endsWith(".sh") || base.endsWith(".bash"))
        return `Running script ${base}`;
      if (base.endsWith(".py")) return `Running ${base}`;
      return "Running a command";
  }
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
    return typeof v === "string" && v.length > 0
      ? v.length > max
        ? "…" + v.slice(-(max - 1))
        : v
      : null;
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
    case "file_list":
    case "file_glob": {
      const p = str("dir", 55) ?? str("pattern", 55) ?? str("path", 55);
      return p ? `Listing ${p}` : fallback;
    }
    case "file_grep": {
      const p = str("pattern", 40);
      return p ? `Searching for "${p}"` : fallback;
    }
    case "bash_executor": {
      const cmd = str("command", 200);
      return cmd ? describeBashCommand(cmd) : fallback;
    }
    case "python_executor": {
      const code = str("code", 60);
      return code ? `Running Python: ${code}` : fallback;
    }
    case "http_fetch":
    case "web_read": {
      const url = str("url", 200);
      if (url) {
        try {
          return `Fetching ${new URL(url).hostname}`;
        } catch {
          return `Fetching ${url}`;
        }
      }
      return fallback;
    }
    case "web_search": {
      const q = str("query", 60);
      return q ? `Searching the web for "${q}"` : fallback;
    }
    case "memory_search": {
      const q = str("query", 60);
      return q ? `Searching memory for "${q}"` : fallback;
    }
    case "ask_user":
      return "Asking the user a question";
    default:
      if (toolName.startsWith("a2a:")) {
        const skill = toolName.slice(4);
        return `Delegating to ${skill}`;
      }
      if (toolName.startsWith("mcp:")) {
        return `Calling external tool ${toolName.slice(4)}`;
      }
      return fallback;
  }
}
