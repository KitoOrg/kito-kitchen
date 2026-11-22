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
kitchen.toml            name = "Shopworks", and the repositories it is for
behaviour.md            how agents work here: always told
concepts/<name>.md      what a word of the team means: looked up
recipes/<name>.md       how something is done, step by step: looked up
tools/<name>.toml       a command every agent has, and what it takes
instructions/all/<name>.md         what every assistant is told, in its own file
instructions/<assistant>/<name>.md the same, for one assistant: claude, codex, zcode
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
  # network = true              # it opens no connection unless it says so

  [input]                       # a JSON Schema of what it takes
  type = "object"
  required = ["only"]
  ```

  What an agent gives is one word of the command, whatever is in it: no
  shell reads it. A tool runs confined to the lane, as `kito lane run`
  runs a command: it writes only in the lane's working copy, and reads
  neither Kito's own data nor your keys.
- **A part** (`parts/rust/`, `parts/web/`) is for the repositories that
  pick it. A repository gets the base and its parts; a part wins over
  the base where both have something of the same name.

- **Instructions** are Kito's standard for what assistants are told. A
  repository the kitchen is attached to (`kito-server repo set <repository>
  --kitchen <kitchen>`) has them written by Kito into `CLAUDE.md` and
  `AGENTS.md`, after Kito's rules and before the repository's own, which
  win where both have a file of the same name (`kito instructions`). Those
  files are not part of the repository: Kito keeps them out of its history
  and of git, and writes them again at every session.

It is changed like code: a lane in the kitchen's repository, a review,
an integration. What its main lane has is the kitchen, from then on, for
everybody. Behaviour, concepts and recipes are asked of the kitchen by the
agents, and instructions are written again at each session, so nothing
goes stale.

## Setting it up

On the server:

```console
$ kito-server kitchen new ./kitchen "Shopworks"      # files to start from
$ kito-server repo add kitchen --from <a git repository of them>
$ kito-server plugin add --path plugin
$ kito-server plugin enable kitchen --for-clients
$ kito-server kitchen set kitchen                    # the kitchen of every repository
$ kito-server repo set shop --kitchen shopkitchen    # or of one: another repository
$ kito-server kitchen parts shop rust web            # what `shop` picks
$ kito-server kitchen show shop                      # what it gets, of which kitchen, and why
$ kito-server kitchen check                          # which kitchen each gets, and what is wrong
```

For each person, once: `kito plugin add --path plugin`. In a directory
connected to the server there is nothing to enable: the server did.

## Which kitchen a repository gets

A server can have one kitchen for all its repositories (`kito-server
kitchen set`), and a repository can have one of its own: the kitchen its
own configuration names, which is Kito's setting (`kito-server repo set
<repository> --kitchen <kitchen>`, or the repository's settings in the web
client).

- A repository gets the kitchen it names, when that is a repository of
  the server; else the server's.
- The repository of a kitchen gets that kitchen: whoever works on a
  kitchen sees it.
- The parts a repository picks are those of the kitchen it gets.

A kitchen can say which repositories it was made for:

```toml
# kitchen.toml
name = "Shopworks"
about = "How we work on the shop."
for = ["shop", "books"]
```

The web client writes `for` when a kitchen is created there; `kitchen
new` does not. It gives the kitchen to nobody: which kitchen a repository
gets is the repository's to say, or the server's. It is what a draft is
written from (below), and `kito-server kitchen check` reports the names in
it that are not repositories of the server.

## Drafting it with your assistant

A kitchen does not have to be written by hand. Connect a directory to the
kitchen's own repository, and start your assistant there. When the kitchen
has nothing in it yet (no behaviour, concept, recipe, tool or
instruction), the agent is told so when its session starts, and, when you
maintain that repository, offers to draft it with you. It calls the tool
`draft`, which gives it an interview to lead:

1. it says what it found in the repositories the kitchen is for;
2. it asks how the team works, one question at a time, and skips what the
   repositories already answer;
3. it proposes as tools only commands those repositories have written
   down, one by one, and writes those you say yes to;
4. it writes everything in one lane of the kitchen's repository;
5. it shows you the files, fixes what is wrong, and asks for the
   integration: the lane can be read and commented in the web client
   first, and the repositories get the kitchen once it is integrated.

On a kitchen that has content the same tool revises it: ask your
assistant to, and it changes only what you say.

`draft` answers only in a kitchen's own repository, and only whoever
maintains that repository (the role `maintain`, which the server's admins
have everywhere). Of each repository named in `for` that uses this
kitchen, it reads the main directory, as integrated:

- **what the repository already tells assistants**: its own instructions
  (`kito instructions`), and `CLAUDE.md`, `AGENTS.md`, `.cursorrules` and
  `.github/copilot-instructions.md` without the part Kito writes in them;
- **what says how it is worked on**: `README*`, `CONTRIBUTING*`,
  `package.json`, `Makefile`, `justfile`, `Cargo.toml`, `pyproject.toml`,
  `go.mod` and the workflows in `.github/workflows`, each cut at 4000
  characters; and the names, not the text, of the lint and format
  configuration at its root (`.eslintrc*`, `eslint.config.*`,
  `.prettierrc*`, `rustfmt.toml`, `clippy.toml`, `ruff.toml`,
  `.editorconfig`, `tsconfig.json`);
- **the commands those files name**, each with its file: the scripts of
  `package.json` (`npm run <name>`), the targets of the `Makefile`, the
  recipes of the `justfile`, and the `run:` lines of the workflows (a
  block of more than three lines is a script, and is left out). None is
  guessed from the language: `cargo test` is not there unless a file
  says it;
- **its layout**: the directories down to two levels and the files at
  the root, 80 at most.

It never reads or names:

- anything under `.git`, `node_modules`, `target`, `dist`, `build` or
  `vendor`, nor of Kito's own data (`.kito`) anything but those
  instructions;
- a file whose name starts with `.env`;
- a name with `secret`, `credential`, `password`, `token`, `id_rsa` or
  `id_ed25519` in it, or that ends in `.pem`, `.key`, `.p12` or `.pfx`;
- a file that is not UTF-8 text;
- a symbolic link, which it does not follow.

An answer stays under about 150 KB: once a file has no room, no other is
added, and those that are not there are named in it (`left_out`). A
repository in `for` that does not use the kitchen is not read, and that
is said too. Nothing of a draft is kept on your machine.

## For agents

Kito offers the plugin's tools next to its own (`kito-mcp`), to every
assistant it is set up for:

- `kitchen`: the behaviour, and what concepts, recipes and tools there are;
- `concept`, `recipe`: the text of one;
- `run`: one of the kitchen's tools, in the lane the agent works in;
- `draft`: in a kitchen's own repository, the interview to draft or revise
  it by.

When a session starts the agent is told the behaviour and the lists
without asking, after Kito's own rules; in the repository of a kitchen
that has nothing in it yet, it is told to offer its maintainer to draft
it.

What they answer says whose words these are: the team's instructions,
to follow, and not Kito's own rules, which stand whatever a kitchen says.

## When the server is away

What the server answered is kept in the directory. When it cannot be
reached, the kitchen is the one it last gave, and says how old it is;
what was never asked for is not there to give.

## What to know before a team uses it

- **A kitchen's tool runs confined to the lane** where Kito can confine
  a command (Linux 5.13 or later): it writes only there, reads nothing
  of Kito's own or of your keys, and reaches the network only if the
  kitchen says the tool needs it. It still reads the rest of your
  machine, and runs as you: whoever may change the kitchen decides what
  runs, so give that role accordingly and review changes to `tools/`
  like code.
- **Where Kito cannot confine a command** (macOS, Windows, an older
  Linux) a tool is not run, and says so. `KITO_KITCHEN_UNCONFINED=1`
  runs the kitchen's tools as yourself there, unconfined: your call, on
  your machine. `KITO_KITCHEN_NO_TOOLS=1` turns them off altogether. A
  kitchen's scripts are for a Unix shell.
- Which repository is the server's kitchen and what each repository
  picks are kept by the plugin on the server
  (`plugins/kitchen/config.toml`), not in a repository: they have no
  history. The kitchen a repository names for itself is in that
  repository's own configuration, which Kito keeps.
- Everybody who may use a repository of the server may read the kitchen
  it gets.
- **Whoever maintains a kitchen is given by `draft` what the repositories
  that use it say of how they are worked on** (the files listed above),
  whether or not they may read those repositories themselves: as with
  its tools, a repository trusts whoever may change the kitchen it uses.
  A repository that does not use the kitchen is never read, whatever
  `for` says.
- It has been run by its tests and Kito's, under Kito itself; not yet by
  a team, with an assistant in a real session.

## Development

```console
$ cargo test
```

The last test runs the plugin under Kito itself, server and client, and
wants `kito`, `kitod` and `kito-server` (0.29 or later) in `KITO_BIN` or
in `../kito/target/debug`.

## License

MIT or Apache-2.0, at your option.
