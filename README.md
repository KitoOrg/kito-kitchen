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
parts/<part>/…          the same, for the repositories that pick the part
```

- **Behaviour** is short, and every agent is given all of it.
- **Concepts** and **recipes** are named. The first line of each file
  says what it is about; an agent is told the names and those lines, and
  asks for the text when it needs it. A kitchen can be large without
  being in every prompt.
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

- `kitchen`: the behaviour, and what concepts and recipes there are;
- `concept`, `recipe`: the text of one.

What they answer says whose words these are: the team's instructions,
to follow, and not Kito's own rules, which stand whatever a kitchen says.

## What is not there yet

- **Tools** of a kitchen (commands every agent has), the behaviour given
  when a session starts without the agent asking, and a copy kept for
  when the server cannot be reached: the next part of the work.
- Which repository is the kitchen and what each repository picks are
  kept by the plugin on the server (`plugins/kitchen/config.toml`), not
  in a repository: they have no history.
- Everybody who may use a repository of the server may read the kitchen.

## Development

```console
$ cargo test
```

The last test runs the plugin under Kito itself, server and client, and
wants `kito`, `kitod` and `kito-server` (0.22 or later) in `KITO_BIN` or
in `../kito/target/debug`.

## License

MIT or Apache-2.0, at your option.
