# Apollia OS, agent examples

Small reference agents, meant to be read in one sitting and copied as the
starting point for your own work.

## What is here

| Example | Pattern |
|---|---|
| `hello/` | The minimal contract: `@agent` plus one `@on_message` handler. |

## Run it

```sh
apollia-os agent install agents/examples/hello
apollia-os agent enable hello
apollia-os run hello "hello from Apollia"
```

`install` accepts a directory holding a `manifest.toml`, a single `.py` file,
or a Git URL. Without the `enable` step, `run` fails with
`agent not found: hello`.

To talk to it in the chat REPL instead, start the daemon and run
`apollia-os chat`.

## Write your own

1. Copy `hello/` and rename the directory.
2. Edit `manifest.toml`: `name`, `version`, `description`, and `entry` if you
   rename the Python file. The name there and the one in the `@agent`
   decorator must match.
3. Replace the handler. Use `@on_message` for a conversational agent, or
   `@skill` for one that another agent calls by skill name.
4. Do not write `agent = YourClass()` yourself. `@agent` instantiates the class
   and binds the instance to the module; adding the line builds a second
   instance that overwrites the registered one.

The authority on what the runtime hands your agent is the type contract in
`sdk/apollia/types.py` and `sdk/apollia/context/`. Read it rather than
inferring from an example: an example shows shape, the types say what is
guaranteed.

## Questions

Usage questions belong in Discussions Q&A. A defect in an example is an issue.
