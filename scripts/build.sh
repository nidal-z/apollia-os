#!/usr/bin/env bash
#
# scripts/build.sh — Build dispatcher pour apollia-os (CLI binary).
#
# Compile le binaire `apollia-os` avec le bon couple <target Rust> + <features>
# pour chaque combinaison OS/architecture/accélérateur. Utilise les features
# déclarées dans `crates/apollia-cli/Cargo.toml`.
#
# Usage :
#   ./scripts/build.sh <preset> [--debug] [--check] [--release] [extra cargo args...]
#
# Presets (cf. BUILD.md pour la matrice complète) :
#
#   macos-silicon        # Apple Silicon (M1+) — Metal + Accelerate
#
#   linux-x86-cpu        # Linux x86_64, CPU only
#   linux-x86-cuda       # Linux x86_64 + NVIDIA CUDA
#   linux-x86-rocm       # Linux x86_64 + AMD ROCm/hipBLAS
#   linux-x86-vulkan     # Linux x86_64 + Vulkan (fallback cross-vendor)
#   linux-arm-cpu        # Linux aarch64 (Pi, Graviton, ...), CPU only
#   linux-arm-cuda       # Linux aarch64 + CUDA (NVIDIA Jetson/Orin/Thor)
#
#   windows-x86-cpu      # Windows x86_64, CPU only
#   windows-x86-cuda     # Windows x86_64 + NVIDIA CUDA
#   windows-x86-rocm     # Windows x86_64 + AMD HIP SDK (Radeon Pro / MI uniquement)
#   windows-x86-vulkan   # Windows x86_64 + Vulkan (fallback cross-vendor)
#   windows-arm-cpu      # Windows aarch64 (Snapdragon X), CPU only
#   windows-arm-cuda     # Windows aarch64 + CUDA — THÉORIQUE : aucun driver
#                          NVIDIA discrete pour Windows-on-ARM aujourd'hui.
#                          Le preset existe pour Jetson/Orin sous WSL/cross-build.
#
# Options :
#   --debug              cargo build (non --release)
#   --release            forcer --release (défaut)
#   --check              cargo check seulement (pas de link, validation rapide)
#   --list               imprime la liste des presets et exit
#   -h, --help           cette aide
#
# Env :
#   APOLLIA_TARGET_DIR   override CARGO_TARGET_DIR (sortie par défaut : target/<triple>/<profile>/)
#
# Notes :
# - Le script ne tente PAS de cross-compiler depuis macOS/Linux vers Windows
#   (toolchain MSVC requise). Lance le preset Windows directement depuis une
#   machine Windows (Git Bash, WSL, ou PowerShell via scripts/build.ps1).
# - Idem ARM Linux : cross-build x86→ARM nécessite un cross-toolchain (gcc-aarch64-
#   linux-gnu). Le script échoue avec un hint si le target n'est pas installé.

set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

# ─── Preset table ──────────────────────────────────────────────────────────────
#
# Chaque ligne : "preset|triple|features"
PRESETS=$(cat <<'EOF'
macos-silicon|aarch64-apple-darwin|cloud,local-metal,local-accelerate,stt-metal
linux-x86-cpu|x86_64-unknown-linux-gnu|cloud,local-cpu,stt-whisper-cpp
linux-x86-cuda|x86_64-unknown-linux-gnu|cloud,local-cuda,stt-cuda
linux-x86-rocm|x86_64-unknown-linux-gnu|cloud,local-rocm,stt-rocm
linux-x86-vulkan|x86_64-unknown-linux-gnu|cloud,local-vulkan,stt-whisper-cpp
linux-arm-cpu|aarch64-unknown-linux-gnu|cloud,local-cpu,stt-whisper-cpp
linux-arm-cuda|aarch64-unknown-linux-gnu|cloud,local-cuda,stt-cuda
windows-x86-cpu|x86_64-pc-windows-msvc|cloud,local-cpu,stt-whisper-cpp
windows-x86-cuda|x86_64-pc-windows-msvc|cloud,local-cuda,stt-cuda
windows-x86-rocm|x86_64-pc-windows-msvc|cloud,local-rocm,stt-rocm
windows-x86-vulkan|x86_64-pc-windows-msvc|cloud,local-vulkan,stt-whisper-cpp
windows-arm-cpu|aarch64-pc-windows-msvc|cloud,local-cpu,stt-whisper-cpp
windows-arm-cuda|aarch64-pc-windows-msvc|cloud,local-cuda,stt-cuda
EOF
)

# ─── Args ──────────────────────────────────────────────────────────────────────
preset=""
profile_flag="--release"
verb="build"
extra=()

while [ $# -gt 0 ]; do
    case "$1" in
        --debug)   profile_flag="" ;;
        --release) profile_flag="--release" ;;
        --check)   verb="check" ;;
        --list)
            echo "Available presets:"
            echo "$PRESETS" | awk -F'|' '{ printf "  %-22s  %-30s  features: %s\n", $1, $2, $3 }'
            exit 0
            ;;
        -h|--help)
            sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        --*) extra+=("$1") ;;
        *)
            if [ -z "$preset" ]; then
                preset="$1"
            else
                extra+=("$1")
            fi
            ;;
    esac
    shift
done

if [ -z "$preset" ]; then
    echo "ERROR: missing preset. Run --list to see available presets." >&2
    exit 1
fi

# ─── Resolve preset ────────────────────────────────────────────────────────────
row=$(echo "$PRESETS" | awk -F'|' -v p="$preset" '$1 == p { print; exit }')
if [ -z "$row" ]; then
    echo "ERROR: unknown preset '$preset'." >&2
    echo "       Run: $(basename "$0") --list" >&2
    exit 1
fi

triple=$(echo "$row" | awk -F'|' '{print $2}')
features=$(echo "$row" | awk -F'|' '{print $3}')

# ─── Detect host and choose runner (native cargo / cross / cargo-xwin) ─────────
#
# Cross-compile reality :
# - Pure CPU presets (-cpu, -vulkan) sont cross-compilables depuis n'importe
#   quel host avec `cross` (Linux targets) ou `cargo-xwin` (Windows targets).
# - Presets GPU (-cuda, -rocm) requièrent le SDK propriétaire (CUDA Toolkit,
#   HIP SDK) — qui n'est PAS disponible sur macOS. Build natif obligatoire.
# - Vulkan : SPIR-V shaders compilent depuis n'importe où mais le link Vulkan
#   nécessite le loader runtime de la plateforme cible. `cross` gère ça pour
#   Linux ; `cargo-xwin` ne le gère pas par défaut → on accepte mais on warn.
host_os="$(uname -s)"
host_native_target=""
case "$host_os" in
    Darwin)             host_native_target="aarch64-apple-darwin" ;;
    Linux)              host_native_target="$(uname -m | sed 's/x86_64/x86_64/;s/aarch64/aarch64/')-unknown-linux-gnu" ;;
    MINGW*|MSYS*|CYGWIN*) host_native_target="x86_64-pc-windows-msvc" ;;
esac

# Determine if this is a native build or a cross-compile.
is_cross=false
[ "$triple" != "$host_native_target" ] && is_cross=true

# Which cargo invocation to use.
runner="cargo"

if $is_cross; then
    case "$preset" in
        macos-*)
            echo "ERROR: '$preset' must be built on a macOS host (not '$host_os')." >&2
            exit 1
            ;;
        linux-*-cuda|linux-*-rocm)
            case "$host_os" in
                Linux) : ;;
                *)
                    echo "ERROR: '$preset' requires a native Linux host with the GPU
       SDK installed (CUDA Toolkit / ROCm). Cross-compile from $host_os is
       not supported — the SDK doesn't run on macOS/Windows.
       → Use GitHub Actions (.github/workflows/release-cli-binaries.yml)
         or SSH into a Linux machine." >&2
                    exit 1
                    ;;
            esac
            ;;
        windows-*-cuda|windows-*-rocm)
            case "$host_os" in
                MINGW*|MSYS*|CYGWIN*) : ;;
                *)
                    echo "ERROR: '$preset' requires a native Windows host with the GPU
       SDK installed (CUDA Toolkit / HIP SDK). Cross-compile from $host_os
       is not supported.
       → Use GitHub Actions or a Windows VM." >&2
                    exit 1
                    ;;
            esac
            ;;
        linux-*)
            # CPU or Vulkan — cross-compile via `cross` (Docker).
            if ! command -v cross >/dev/null 2>&1; then
                echo "ERROR: cross-compile Linux target from $host_os requires 'cross'.
       Install:
         cargo install cross --git https://github.com/cross-rs/cross
       And Docker (or Podman/Colima) running.

       Alternative: use GitHub Actions or run this script on a native Linux host." >&2
                exit 1
            fi
            if ! docker info >/dev/null 2>&1 && ! podman info >/dev/null 2>&1; then
                echo "ERROR: cross needs Docker/Podman running. Start it then retry." >&2
                exit 1
            fi
            runner="cross"
            ;;
        windows-*)
            # CPU or Vulkan — cross-compile via `cargo-xwin` (fetches MSVC SDK).
            if ! command -v cargo-xwin >/dev/null 2>&1 \
                && ! cargo --list 2>/dev/null | grep -q xwin; then
                echo "ERROR: cross-compile Windows target from $host_os requires 'cargo-xwin'.
       Install:
         cargo install cargo-xwin
       It will fetch the MSVC SDK from Microsoft's CDN on first build (~700 MB,
       cached at ~/.cache/cargo-xwin/).
       Note : LLVM/clang must be on PATH (brew install llvm sur macOS).

       Alternative: use GitHub Actions or run this script on a native Windows host." >&2
                exit 1
            fi
            if ! command -v clang >/dev/null 2>&1; then
                echo "ERROR: cargo-xwin requires clang on PATH. Install LLVM:
         brew install llvm
       Then export: export PATH=\"\$(brew --prefix llvm)/bin:\$PATH\"" >&2
                exit 1
            fi
            runner="cargo xwin"
            ;;
    esac
fi

# Reject ARM-Windows + CUDA loudly (theoretical preset).
if [ "$preset" = "windows-arm-cuda" ]; then
    echo "WARNING: NVIDIA does NOT ship CUDA drivers for Windows-on-ARM
         (Snapdragon X, etc.). This preset only makes sense if you are
         cross-building from Windows-on-ARM toward a Jetson/Orin Linux
         target via WSL — most likely you want 'linux-arm-cuda' instead.
         Continuing anyway in 5s. Ctrl-C to abort." >&2
    sleep 5
fi

# ─── Ensure rustup target installed (native runner only) ───────────────────────
# `cross` handles the target install inside its Docker image. `cargo-xwin` does
# rustup target add on its own.
if [ "$runner" = "cargo" ] && command -v rustup >/dev/null 2>&1; then
    if ! rustup target list --installed 2>/dev/null | grep -qx "$triple"; then
        echo "→ Installing rustup target: $triple"
        rustup target add "$triple"
    fi
fi

# ─── Build ─────────────────────────────────────────────────────────────────────
echo "─── apollia-os build ───"
echo "  preset    : $preset"
echo "  triple    : $triple"
echo "  features  : $features"
echo "  profile   : ${profile_flag:-debug}"
echo "  verb      : $verb"
echo "  runner    : $runner"
echo "  cross     : $is_cross"
echo "  extra     : ${extra[*]:-(none)}"
echo

set -x
$runner "$verb" \
    -p apollia-cli \
    --target "$triple" \
    --no-default-features \
    --features "$features" \
    ${profile_flag} \
    "${extra[@]}"
set +x

# ─── Locate output ─────────────────────────────────────────────────────────────
profile_dir="debug"
[ "$profile_flag" = "--release" ] && profile_dir="release"
ext=""
case "$triple" in
    *-pc-windows-*) ext=".exe" ;;
esac
bin_path="target/$triple/$profile_dir/apollia-os${ext}"

if [ "$verb" = "build" ] && [ -f "$bin_path" ]; then
    size=$(du -h "$bin_path" | awk '{print $1}')
    echo
    echo "✓ Built: $bin_path  ($size)"
fi
