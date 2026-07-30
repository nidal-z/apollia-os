# ADR-049: Windows is a supported platform for v0.1.0

- Status: Accepted
- Date: 2026-07-30
- Supersedes: the "Platform scope" section of
  [ADR-003](ADR-003-sandbox-trust-platform-scope.md). The trust-model and
  tool-sandbox decisions of ADR-003 stand unchanged.

## Context

ADR-003 excluded Windows from v0.1.0 and v1.0, on the grounds that the Unix
socket API, the `notify-rust` notification backend, the GPU LLM backends and the
POSIX assumptions in the tool sandboxes had "no portable Windows path that fits
the schedule". It explicitly left the door open: Windows "may be reassessed in a
later cycle", with that decision "recorded in a future ADR". This is that ADR.

Two things changed since 2026-06-04.

**The portability cost was overestimated.** Measured on 2026-07-30 by
cross-compiling the workspace for a Windows target: `apollia-core`,
`apollia-tools`, `apollia-permissions`, `apollia-llm`, `apollia-memory`,
`apollia-workspace` and `apollia-runtime` all compile with no source-level
error. A single production blocker existed, an ungated
`std::os::unix::fs::symlink` in the desktop CLI-install command. The Unix socket
and the POSIX primitives were already `cfg`-gated, and `apollia-cli` carries a
correct dual-stack IPC path (Unix socket on Unix, loopback TCP on Windows).
What remains is runtime correctness, not portability of the source.

**The local LLM engine changed shape.** ADR-003 reasoned about GPU backends
compiled into the runtime. Since the llama-server migration the engine is a
pinned upstream binary staged into the bundle, and upstream publishes Windows
builds (CPU, CUDA, Vulkan) alongside the macOS and Linux ones. The GPU argument
for excluding Windows no longer applies: the same fetch-and-verify path serves
all three platforms.

## Decision

Windows x86_64 is a supported platform for v0.1.0, alongside macOS Apple Silicon
and Linux x86_64. Public documentation, the site and the announcement present the
three platforms as the supported set.

Support means, concretely:

- The daemon and CLI build and run on Windows, using loopback TCP for IPC where
  Unix uses a domain socket.
- The user profile resolves through the platform home directory
  (`%USERPROFILE%` on Windows), never through a bare `HOME` read with a POSIX
  fallback.
- The embedded Python bundle and the embedded `llama-server` are located and
  staged with the Windows layout.
- Per-agent virtualenvs use the Windows script directory.
- Credentials persist in the Windows Credential Manager.
- The desktop app builds as `.msi` and NSIS `.exe`.

Two capabilities are explicitly narrower on Windows, and are documented as such
rather than silently degraded:

- **No OS-level tool sandbox.** ADR-003's trust model is unchanged: there is no
  process-per-agent isolation on any platform. On Linux, tool subprocesses get
  PID and mount namespaces plus `setrlimit`; on macOS, `setrlimit` only; on
  Windows, neither. `SecurityPosture` reports this per host instead of implying
  a uniform guarantee.
- **Shell tooling differs.** `bash_executor` assumes a POSIX shell. On Windows it
  requires one on `PATH` (Git Bash, WSL, MSYS2) and refuses with a clear error
  otherwise, rather than silently invoking `cmd.exe` with different quoting,
  different exit-code semantics and a different injection surface than the
  validator was written for.

## Alternatives considered

### Keep Windows out of scope (rejected)

- Pros: no new platform to test; ADR-003 stands untouched.
- Cons: excludes a large share of the addressable audience for a runtime whose
  wedge is embeddability into host applications, most of which ship on Windows.
  The measured cost no longer justifies the exclusion.

### Windows through WSL2 only (rejected, as in ADR-003)

- Pros: reuses the Linux path unchanged, including namespaces.
- Cons: not a native desktop application; requires the user to install and
  manage WSL2; the Tauri app would not run natively. Rejected for the same
  reasons as in ADR-003, which remain valid.

### Ship Windows without a shell executor (rejected)

- Pros: removes the only genuinely non-portable tool.
- Cons: silently different capability set per platform, which is the kind of
  invisible asymmetry this project avoids. Requiring a POSIX shell and saying so
  is more honest and keeps the security validator meaningful.

## Consequences

Positive:

- Three supported platforms from the first public release.
- The home-directory resolution is centralised instead of being 98 scattered
  `HOME` reads with `"/tmp"` fallbacks, which was a latent defect on every
  platform, not only Windows.

Negative and accepted:

- A third platform to validate before each release. The manual matrix gains a
  Windows lane, and the release pipeline gains Windows jobs that have never run
  green.
- Windows has no per-process resource limits and no namespace isolation for tool
  subprocesses. This is a real reduction in defence-in-depth relative to Linux,
  surfaced through `SecurityPosture` and documented in `docs/agents/SECURITY.md`.
- `bash_executor` depends on an external POSIX shell on Windows, which is a
  prerequisite the user must satisfy.

To watch:

- Whether the absence of rlimits on Windows justifies a platform-specific
  mitigation (job objects) in a later cycle.
- Whether `cmd.exe` or PowerShell support for `bash_executor` is worth the
  second validator surface it would require.
