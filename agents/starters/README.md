# Apollia OS, proposed example agents

Each folder here is one declarative package: an `apollia.toml` (mission,
examples, accesses, sources, required connections) and the data files it
refers to. No code.

The runtime embeds every folder of this directory at build time
(`crates/apollia-runtime/build.rs`). The application proposes them in the
Agents screen and installs one only when the user chooses it, after the same
preview as any package. Nothing here is installed on a fresh machine.

## Add an example

1. Create a folder named after the package: `agents/starters/<name>/`.
2. Write its `apollia.toml`, with `[package] name = "<name>"`.
3. Run `cargo test -p apollia-runtime packages::tests_examples`: every folder
   must parse, declare at least one agent, carry no tool, no workflow and no
   Python requirement, and read back once assembled into a `.apo` archive.

A file or folder whose name starts with a dot is left out.

These packages are not the SDK examples: those live in `agents/examples/`.
