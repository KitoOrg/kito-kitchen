# kito-kitchen

A team's **kitchen** for [Kito](https://github.com/KitoOrg/kito): what a
team shares to work on its repositories. How its agents behave, what its
words mean, how things are done here. Written once, kept on the team's
Kito server, and given to every agent of every person, whatever coding
assistant they use.

```console
$ kito kitchen
The kitchen of Shopworks for `shop`, with rust
How we work on the shop.

Say what you ran before you ask for an integration.

## rust

Run `cargo clippy` before the tests.

Concepts (`kito kitchen concept <name>`):
  money                Amounts are `Cents(i64)`  [rust]

Recipes (`kito kitchen recipe <name>`):
  release              How a release is made
```

## What a kitchen is

A repository on the server, with these files:

```text
kitchen.toml            name = "Shopworks"
behaviour.md            how agents work here: always told
concepts/<name>.md      what a word of the team means: looked up
recipes/<name>.md       how something is done, step by step: looked up
tools/<name>.toml       a command every agent has, and what it takes
parts/<part>/…          the same, for the repositories that pick the part
```

- **Behaviour** is short, and every agent is given all of it.
- **Concepts** and **recipes** are named. The first line of each file
  says what it is about; an agent is told the names and those lines, and
  asks for the text when it needs it. A kitchen can be large without
  being in every prompt.
- **A tool** is a command every agent of the team has, run in the
  working copy of the lane it works in:

  ```toml
  # tools/test.toml
  about = "Runs the tests; `only` names one file"
  script = "test.sh"            # a file next to this one, which the kitchen carries
  # command = "npm"             # or a program the machine has
  args = ["{only}"]             # `{name}`: what the agent gave under that name
  timeout_secs = 600

  [input]                       # a JSON Schema of what it takes
  type = "object"
  required = ["only"]
  ```

  What an agent gives is one word of the command, whatever is in it: no
  shell reads it.
- **A part** (`parts/rust/`, `parts/web/`) is for the repositories that
  pick it. A repository gets the base and its parts; a part wins over
  the base where both have something of the same name.

It is changed like code: a lane in the kitchen's repository, a review,
an integration. What its main lane has is the kitchen, from then on, for
everybody. Nothing is copied into `CLAUDE.md` or `AGENTS.md`, so nothing
goes stale there.

## Setting it up

On the server:

```console
$ kito-server kitchen new ./kitchen "Shopworks"      # files to start from
$ kito-server repo add kitchen --from <a git repository of them>
$ kito-server plugin add --path plugin
$ kito-server plugin enable kitchen --for-clients
$ kito-server kitchen set kitchen
$ kito-server kitchen parts shop rust web            # what `shop` picks
$ kito-server kitchen show shop                      # what it gets
$ kito-server kitchen check                          # what is wrong with it
```

For each person, once: `kito plugin add --path plugin`. In a directory
connected to the server there is nothing to enable: the server did.

## For agents

Kito offers the plugin's tools next to its own (`kito-mcp`), to every
assistant it is set up for:

- `kitchen`: the behaviour, and what concepts, recipes and tools there are;
- `concept`, `recipe`: the text of one;
- `run`: one of the kitchen's tools, in the lane the agent works in.

When a session starts the agent is told the behaviour and the lists
without asking, after Kito's own rules.

What they answer says whose words these are: the team's instructions,
to follow, and not Kito's own rules, which stand whatever a kitchen says.

## When the server is away

What the server answered is kept in the directory. When it cannot be
reached, the kitchen is the one it last gave, and says how old it is;
what was never asked for is not there to give.

## What to know before a team uses it

- **A kitchen's tool runs as you**, in the lane's working copy, and is
  not confined. Whoever may change the kitchen may run commands on the
  team's machines: give that role accordingly, and review changes to
  `tools/` like code. `KITO_KITCHEN_NO_TOOLS=1` turns the tools off on a
  machine. A kitchen's scripts are for a Unix shell.
- Which repository is the kitchen and what each repository picks are
  kept by the plugin on the server (`plugins/kitchen/config.toml`), not
  in a repository: they have no history.
- Everybody who may use a repository of the server may read the kitchen.
- It has been run by its tests and Kito's, under Kito itself; not yet by
  a team, with an assistant in a real session.

## Development

```console
$ cargo test
```

The last test runs the plugin under Kito itself, server and client, and
wants `kito`, `kitod` and `kito-server` (0.22 or later) in `KITO_BIN` or
in `../kito/target/debug`.

## License

MIT or Apache-2.0, at your option.
