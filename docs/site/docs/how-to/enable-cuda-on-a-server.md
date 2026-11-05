---
sidebar_position: 8.5
title: Enable CUDA on a server
---

# Enable CUDA on a server

This guide gives the GGUF models of a remote Linux server an NVIDIA card to run
on: it checks the card and its driver, installs the CUDA build of
`llama-server` from the engine channel, makes it the engine in service, and
proves the card is used. A second path does the same on a server with no
network. It assumes the daemon already runs as a service, as in
[Deploy in production](/how-to/deploy-in-production), under the `apollia`
user whose home is `/var/lib/apollia`.

The `engine` commands talk to the daemon over its socket, which lives in the
data directory of the user who runs it. Run them as that user, with its home:

```sh
sudo -u apollia -H apollia-os engine list
```

## What the card and the driver need

The engine channel carries two CUDA builds, `cuda-12` and `cuda-13`, beside
`rocm`, `vulkan` and `cpu`. Each CUDA package carries the CUDA runtime
libraries that upstream llama.cpp publishes for its build, placed next to the
server, on Linux as on Windows: the server needs the NVIDIA driver and nothing
else from the machine, no CUDA toolkit.

The runtime rates every variant for the machine, `recommended`, `compatible`
or `incompatible`, from the cards it detects and their driver, and recommends
one at most:

| Variant | Card | Driver for `recommended` | Driver for `compatible` |
|---|---|---|---|
| `cuda-13` | compute capability 7.5 or later | R615 or later | R580 or later |
| `cuda-12` | any NVIDIA card, Maxwell, Pascal and Volta included | 570.26 or later under Linux, 551.61 or later under Windows | 525 under Linux, 527.41 under Windows, on a card with native code in the build |

When both CUDA builds qualify, `cuda-13` is the one recommended. In detail:

- **Maxwell, Pascal and Volta cards run CUDA 12 only.** CUDA 13 dropped their
  architectures, and the runtime rates `cuda-13` incompatible on a card under
  compute capability 7.5, with that reason.
- **A driver below the line is a reason, not a guess.** Under R580, `cuda-13`
  is incompatible; between R580 and R615 it is compatible, and `cuda-12` is
  recommended when its own driver line is met. Below that line, `cuda-12`
  stays compatible down to the floor of the CUDA 12 family on a card the build
  carries native code for (compute capability 8.6 to 8.9, and 12.0 under
  Linux), and is incompatible on any other card, which the build serves through
  PTX and which needs the driver of the toolkit.
- **CUDA 12 on the newest cards.** On a card of compute capability 10.0 or
  later that the build has no native code for (every such card under Windows,
  all but 12.0 under Linux), `cuda-12` is at best compatible.

When the machine has several NVIDIA cards, the CUDA builds are rated for the
one with the highest compute capability, then the most memory.

## Check what the runtime sees

```sh
sudo -u apollia -H apollia-os engine check
sudo -u apollia -H apollia-os engine list
```

`engine check` probes the cards again and reads the followed channel, `stable`
unless `engine channel` says otherwise. The runtime reads the channel from
`https://registry.apollia.fr/engines-<channel>/`, then, when the registry does
not answer, from the GitHub release `engines-<channel>` of the project, which
mirrors it. Neither address is trusted: the manifest `engines.json` is
accepted only when its signature `engines.json.minisig` holds against the
distribution key compiled into the binary, and a build that carries no such key
refuses every engine. A channel out of reach or a refused signature ends
`engine check` with exit 2 and the reason.

`engine list` prints four blocks: the cards detected (name, driver, memory,
compute capability), the engines installed with the one in service, the
variants the channel offers for this platform with their verdict and reason,
and Node.js. Variants that do not run here are counted rather than listed;
`engine list --all` lists them with their reason, for instance
`driver 560 < 580` or `compute capability 6.1 is not built by CUDA 13`.

## Install the CUDA engine

```sh
sudo -u apollia -H apollia-os engine install llama-server
```

Without a variant, `engine install` takes the one rated recommended for this
machine. Name it to choose another, `engine install llama-server cuda-12`; a
variant rated incompatible is refused unless `--force` is given. The command
follows the installation until it ends: the signature of the manifest is
verified before any archive is requested, and an archive whose SHA-256 differs
from the manifest is deleted. `--detach` returns once the runtime accepted the
installation, and `engine list` then follows it.

## Make it the engine in service

The engine of GGUF models is the `engines.gguf` setting, `auto` by default:
the automatic choice takes the installed build rated recommended, otherwise the
best compatible one, otherwise the engine shipped with the binary. To pin the
CUDA build instead:

```sh
sudo -u apollia -H apollia-os engine use cuda-13
```

`engine use` accepts `auto`, `embedded`, or a variant installed; `cuda` names
the best CUDA build installed. A variant that is not installed is refused, an
installed one rated incompatible is written with a warning. The choice applies
the next time a model is loaded, without a restart of the service. A chosen
variant that is later removed falls back to the automatic choice.

## Verify that the card is used

```sh
sudo -u apollia -H apollia-os engine list
```

The build in service carries `in service` in the Installed block. The runtime
also runs `--list-devices` on every installed GPU build: a build that lists no
device is rated incompatible, with the reason
`the installed build sees no device`. To read that listing yourself, run the
binary of the build, which sits under the data directory of the service user:

```sh
sudo -u apollia -H find /var/lib/apollia/.apollia/engines/llama-server -name llama-server -type f
sudo -u apollia -H <path printed above> --list-devices
```

A CUDA build that sees the card prints one line per device, of the shape
`CUDA0: <name> (<total> MiB, <free> MiB free)`. Then load a model and read the
journal of the daemon: `llama.server.device` names the device the engine
loaded on, and `llama.server.offload` the layers placed on it against the
model's total.

## Without a network

A server that reaches neither address installs the same packages from a
directory, through the same checks. On a machine with a network, copy from the
release `engines-stable` of the project (or `engines-preview`, for the preview
channel) into one directory:

- the manifest, `engines.json`;
- its signature, `engines.json.minisig`;
- the archive of each package to install, named
  `<engine>-<tag>-<variant>-<platform>.zip`, for instance the `llama-server`
  archive of `cuda-13` for `linux-x64`.

The manifest lists every package with its engine, tag, variant, platform,
address, size and SHA-256, so the archive that matches the server is the one
whose `platform` and `variant` it names. Bring the directory to the server
where the service user can read it, then:

```sh
sudo -u apollia -H apollia-os engine import /srv/engines-stable
```

The daemon verifies the signature before it reads any archive, refuses a
manifest older than the last one it accepted on that channel, and installs
every package of the manifest for this platform whose archive is in the
directory, after checking its digest. A directory without a manifest, with a
refused signature, or with an archive that does not match is refused with exit
2. Then `engine use` and the verification above apply unchanged.

## An AMD card under Linux

The `rocm` variant serves AMD cards whose gfx target is in the list of the
upstream build. Under Linux it carries no `libamdhip64`: it loads the ROCm
runtime of the host, so ROCm has to be installed on the server for it to start.
The runtime looks for `libamdhip64` where the dynamic loader looks (the ROCm
install directory, the directories `ld.so.conf` names, `LD_LIBRARY_PATH`, the
default library directories). Without it, `rocm` is rated incompatible with
the reason `no ROCm runtime on this machine`, and Vulkan is the variant
recommended for the card, which `engine install llama-server` then takes.

After installing ROCm on the host, `engine check` probes the machine again, and
`engine list` shows the new verdict; `engine list --all` lists `rocm` with its
reason while it stays incompatible.

## Related

- [Get the most from local inference](/how-to/accelerate-local-inference) for
  the engines the installers embed and the variables that tune the engine.
- [Deploy in production](/how-to/deploy-in-production) for the service this
  guide extends.
- The [CLI reference](/reference/cli) for every `engine` command and its exit
  codes.
