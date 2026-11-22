//! `kito-kitchen`: on a server, and in a directory connected to it.
//!
//! The last test runs it under Kito itself, and needs `kito`, `kitod` and
//! `kito-server` in `KITO_BIN` or in `../kito/target/debug` (`cargo build
//! -p kito-cli -p kitod -p kito-server` there).

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The files of a small kitchen, with a part.
fn stock(dir: &Path) {
    write(
        dir,
        "kitchen.toml",
        "name = \"Shopworks\"\nabout = \"How we work on the shop.\"\n",
    );
    write(
        dir,
        "behaviour.md",
        "Say what you ran before you ask for an integration.\n",
    );
    write(
        dir,
        "concepts/money.md",
        "# Amounts are whole cents\n\nNever a float.\n",
    );
    write(
        dir,
        "recipes/release.md",
        "# How a release is made\n\n1. Tag.\n2. Tell the channel.\n",
    );
    write(
        dir,
        "parts/rust/behaviour.md",
        "Run `cargo clippy` before the tests.\n",
    );
    write(
        dir,
        "parts/rust/concepts/money.md",
        "# Amounts are `Cents(i64)`\n\nSee `crates/money`.\n",
    );
    // A script the kitchen carries, and a command the machine has.
    write(
        dir,
        "tools/greet.sh",
        "#!/bin/sh\necho \"hello $1, from the lane's $(basename \"$PWD\")\"\nls\n",
    );
    write(dir, "tools/greet.toml", "about = \"Says hello, and what is here\"\nscript = \"greet.sh\"\nargs = [\"{who}\"]\n\n[input]\ntype = \"object\"\nrequired = [\"who\"]\n");
    write(
        dir,
        "tools/fail.toml",
        "about = \"Ends badly\"\ncommand = \"sh\"\nargs = [\"-c\", \"echo no; exit 3\"]\n",
    );
    write(
        dir,
        "tools/slow.toml",
        "about = \"Never ends\"\ncommand = \"sleep\"\nargs = [\"30\"]\ntimeout_secs = 1\n",
    );
    // One that tries to write next to the repository, and to read Kito's own files.
    let escape = "echo x > {out}/escaped && echo wrote; cat {out}/.kito/remote.toml > /dev/null && echo read; echo over";
    write(dir, "tools/escape.toml", &format!("about = \"Tries to leave the lane\"\ncommand = \"sh\"\nargs = [\"-c\", \"{escape}\"]\n"));
}

/// Runs the plugin as a server runs it: its directory in the environment,
/// and for a question from a client, the repository asked from and what
/// whoever asks may do in it.
fn on_server(
    dir: &Path,
    args: &[&str],
    asked: Option<(&str, &str)>,
    input: Option<&Value>,
) -> (bool, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kito-kitchen"));
    command
        .args(args)
        .env("KITO_SERVER_DIR", dir)
        .env_remove("KITO_SOCKET")
        .env_remove("KITO_PLUGIN_KEY");
    if let Some((repo, role)) = asked {
        command
            .env("KITO_ASKED_REPO", repo)
            .env("KITO_ASKED_BY", "ada")
            .env("KITO_ASKED_ROLE", role);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    if let Some(input) = input {
        stdin.write_all(input.to_string().as_bytes()).unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// What somebody who may `role` in `repo` is answered from there.
fn answer_as(dir: &Path, question: &str, repo: &str, role: &str, params: Value) -> Value {
    let asked = Some((repo, role));
    let (ok, said) = on_server(dir, &["answer", question], asked, Some(&params));
    assert!(ok, "{said}");
    serde_json::from_str(said.lines().last().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {said}"))
}

fn answer(dir: &Path, question: &str, repo: &str, params: Value) -> Value {
    answer_as(dir, question, repo, "write", params)
}

#[test]
fn a_server_says_which_repository_is_its_kitchen_and_what_each_one_gets() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    for repo in ["kitchen", "shop", "books"] {
        std::fs::create_dir_all(dir.join("repos").join(repo).join(".kito")).unwrap();
    }
    let admin = |args: &[&str]| on_server(dir, args, None, None);

    // No kitchen yet: said to whoever asks, on the server and from a client.
    let (ok, said) = admin(&["kitchen", "show"]);
    assert!(
        !ok && said.contains("this server has no kitchen: `kito-server kitchen set <repository>`"),
        "{said}"
    );
    let none = answer(dir, "kitchen", "shop", json!({}));
    assert!(
        none["error"]
            .as_str()
            .unwrap()
            .contains("this server has no kitchen"),
        "{none}"
    );

    // A repository is the kitchen once it has one in it.
    let (ok, said) = admin(&["kitchen", "set", "kitchen"]);
    assert!(
        !ok && said.contains("is not a kitchen: it has no kitchen.toml"),
        "{said}"
    );
    let (ok, said) = admin(&["kitchen", "set", "nowhere"]);
    assert!(
        !ok && said.contains("this server has no repository `nowhere`"),
        "{said}"
    );
    assert!(!admin(&["kitchen", "set", "../repos/kitchen"]).0);
    stock(&dir.join("repos/kitchen"));
    let (ok, said) = admin(&["kitchen", "set", "kitchen"]);
    assert!(
        ok && said.contains("is the repository `kitchen`: Shopworks."),
        "{said}"
    );
    assert!(
        said.contains("1 concept and 1 recipe, 1 part to pick."),
        "{said}"
    );

    // A repository gets the base, and the parts picked for it.
    let (ok, said) = admin(&["kitchen", "parts", "shop", "go"]);
    assert!(
        !ok && said.contains("the kitchen has no part `go`: it has rust"),
        "{said}"
    );
    let (ok, said) = admin(&["kitchen", "parts", "shop", "rust"]);
    assert!(
        ok && said.contains("`shop` gets the kitchen's base and: rust."),
        "{said}"
    );
    let (ok, said) = admin(&["kitchen"]);
    assert!(
        ok && said.contains("Parts: rust") && said.contains("shop picked rust"),
        "{said}"
    );
    assert_eq!(gets(&said, "books"), "`kitchen` (Shopworks): the server's");
    let (ok, said) = admin(&["kitchen", "show", "shop"]);
    assert!(
        ok && said.contains("the base and rust")
            && said.contains("Amounts are `Cents(i64)`  [rust]"),
        "{said}"
    );
    assert!(admin(&["kitchen", "check"])
        .1
        .contains("The kitchen is in order."));

    // What the same plugin in a connected directory is answered.
    let shop = answer(dir, "kitchen", "shop", json!({}));
    assert_eq!(
        (shop["kitchen"].as_str(), shop["repository"].as_str()),
        (Some("Shopworks"), Some("shop"))
    );
    assert_eq!(shop["parts"], json!(["rust"]));
    assert_eq!(
        shop["behaviour"],
        "Say what you ran before you ask for an integration.\n\n## rust\n\nRun `cargo clippy` before the tests."
    );
    assert_eq!(
        shop["concepts"],
        json!([{ "name": "money", "about": "Amounts are `Cents(i64)`", "part": "rust" }])
    );
    assert_eq!(
        shop["recipes"],
        json!([{ "name": "release", "about": "How a release is made" }])
    );
    let tools: Vec<&str> = shop["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(tools, ["escape", "fail", "greet", "slow"]);
    assert_eq!(shop["tools"][2]["input"]["required"], json!(["who"]));
    // A tool is given with what it takes to run it: the script the kitchen carries.
    let greet = answer(dir, "tool", "shop", json!({ "name": "greet" }));
    assert_eq!(
        (greet["script"]["name"].as_str(), greet["args"][0].as_str()),
        (Some("greet.sh"), Some("{who}"))
    );
    assert!(greet["script"]["text"]
        .as_str()
        .unwrap()
        .starts_with("#!/bin/sh"));
    assert!(
        answer(dir, "tool", "shop", json!({ "name": "nope" }))["error"]
            .as_str()
            .unwrap()
            .contains("it has escape, fail, greet, slow")
    );
    let books = answer(dir, "kitchen", "books", json!({}));
    assert_eq!(books["parts"], json!([]));
    assert_eq!(books["concepts"][0]["about"], "Amounts are whole cents");

    let money = answer(dir, "concept", "shop", json!({ "name": "Money" }));
    assert_eq!(
        money["text"],
        "# Amounts are `Cents(i64)`\n\nSee `crates/money`.\n"
    );
    assert_eq!(
        answer(dir, "concept", "books", json!({ "name": "money" }))["text"],
        "# Amounts are whole cents\n\nNever a float.\n"
    );
    assert!(
        answer(dir, "recipe", "shop", json!({ "name": "release" }))["text"]
            .as_str()
            .unwrap()
            .contains("2. Tell the channel.")
    );
    // What is not there is said with what is; a name is a name, not a path.
    let none = answer(dir, "recipe", "shop", json!({ "name": "deploy" }));
    assert_eq!(
        none["error"],
        "the kitchen has no recipe `deploy`: it has release"
    );
    let out = answer(dir, "concept", "shop", json!({ "name": "../kitchen" }));
    assert!(
        out["error"]
            .as_str()
            .unwrap()
            .contains("the kitchen has no concept"),
        "{out}"
    );
    assert!(answer(dir, "menu", "shop", json!({}))["error"]
        .as_str()
        .unwrap()
        .contains("does not know the question"));

    // What is wrong with a kitchen is found by whoever checks it.
    write(&dir.join("repos/kitchen"), "recipes/empty.md", "\n");
    std::fs::remove_dir_all(dir.join("repos/kitchen/parts/rust")).unwrap();
    let (ok, said) = admin(&["kitchen", "check"]);
    assert!(!ok && said.contains("2 problems"), "{said}");
    assert!(
        said.contains("recipes/empty.md: it says nothing")
            && said.contains("shop: the part `rust` is picked"),
        "{said}"
    );
    // The rest is still given.
    assert_eq!(
        answer(dir, "kitchen", "shop", json!({}))["recipes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (ok, said) = admin(&["kitchen", "parts", "shop"]);
    assert!(ok && said.contains("and no part"), "{said}");

    // A kitchen to start from.
    let new = dir.join("starter");
    let (ok, said) = admin(&["kitchen", "new", new.to_str().unwrap(), "Our", "team"]);
    assert!(ok && said.contains("6 files"), "{said}");
    assert!(std::fs::read_to_string(new.join("kitchen.toml"))
        .unwrap()
        .contains("name = \"Our team\""));
}

/// Writes in a repository's own configuration which kitchen it names, as
/// Kito does (`kito-server repo set <repository> --kitchen`).
fn names(dir: &Path, repo: &str, path: &str) {
    let config = format!("[main]\nprotected = true\n\n[kitchen]\npath = '{path}'\n");
    write(&dir.join("repos").join(repo), ".kito/config.toml", &config);
}

/// What the server says a repository gets, in `Which kitchen each
/// repository gets`.
fn gets(said: &str, repo: &str) -> String {
    let (_, listed) = said
        .split_once("Which kitchen each repository gets:\n")
        .unwrap_or_else(|| panic!("{said}"));
    let line = listed
        .lines()
        .find(|line| line.split_whitespace().next() == Some(repo))
        .unwrap_or_else(|| panic!("{said}"));
    line.trim().strip_prefix(repo).unwrap().trim().to_owned()
}

#[test]
fn a_repository_gets_the_kitchen_it_names_and_else_the_server_s() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    for repo in ["kitchen", "pantry", "bare", "shop", "books", "notes"] {
        std::fs::create_dir_all(dir.join("repos").join(repo).join(".kito")).unwrap();
    }
    let admin = |args: &[&str]| on_server(dir, args, None, None);
    let kitchen_of = |repo: &str| answer(dir, "kitchen", repo, json!({}));
    stock(&dir.join("repos/kitchen"));
    let pantry = dir.join("repos/pantry");
    write(
        &pantry,
        "kitchen.toml",
        "name = \"Pantry\"\nabout = \"How the shop is worked on.\"\nfor = [\"shop\", \"gone\"]\n",
    );
    write(&pantry, "behaviour.md", "Ask before you delete.\n");
    write(
        &pantry,
        "parts/web/concepts/page.md",
        "# A page is a route\n",
    );

    // With no kitchen for the whole server, a repository has the one its
    // own configuration names, and the others have none.
    names(dir, "shop", "../pantry");
    let shop = kitchen_of("shop");
    assert_eq!(
        (shop["kitchen"].as_str(), shop["behaviour"].as_str()),
        (Some("Pantry"), Some("Ask before you delete."))
    );
    assert!(
        shop.get("own").is_none() && shop.get("empty").is_none(),
        "{shop}"
    );
    let none = kitchen_of("books");
    assert!(
        none["error"]
            .as_str()
            .unwrap()
            .contains("this server has no kitchen"),
        "{none}"
    );
    let (ok, said) = admin(&["kitchen", "show"]);
    assert!(
        ok && said.contains("No kitchen for the whole server"),
        "{said}"
    );
    assert_eq!(gets(&said, "shop"), "`pantry` (Pantry): its own setting");
    assert_eq!(gets(&said, "books"), "none");

    // Else it has the server's.
    assert!(admin(&["kitchen", "set", "kitchen"]).0);
    assert_eq!(kitchen_of("books")["kitchen"], "Shopworks");
    assert_eq!(kitchen_of("shop")["kitchen"], "Pantry");
    // What it names is a repository of this server, whatever way the path
    // takes to it; anything else is not its kitchen.
    for path in ["../pantry/", "./../pantry", pantry.to_str().unwrap()] {
        names(dir, "books", path);
        assert_eq!(kitchen_of("books")["kitchen"], "Pantry", "{path}");
    }
    for path in ["../nowhere", "/nowhere/else", "..", "../pantry/parts", ""] {
        names(dir, "books", path);
        assert_eq!(kitchen_of("books")["kitchen"], "Shopworks", "{path}");
    }

    // A kitchen asked about itself gets itself, whatever it names and
    // whichever the server's is.
    names(dir, "pantry", "../kitchen");
    let own = kitchen_of("pantry");
    assert_eq!(
        (
            own["kitchen"].as_str(),
            own["own"].as_bool(),
            own.get("empty")
        ),
        (Some("Pantry"), Some(true), None)
    );
    let own = kitchen_of("kitchen");
    assert_eq!(
        (own["kitchen"].as_str(), own["own"].as_bool()),
        (Some("Shopworks"), Some(true))
    );
    // One with nothing in it yet says so, to whoever gets it.
    let bare = dir.join("repos/bare");
    write(&bare, "kitchen.toml", "name = \"Bare\"\n");
    let own = kitchen_of("bare");
    assert_eq!(
        (own["own"].as_bool(), own["empty"].as_bool()),
        (Some(true), Some(true))
    );
    names(dir, "notes", "../bare");
    let notes = kitchen_of("notes");
    assert_eq!(
        (
            notes["kitchen"].as_str(),
            notes.get("own"),
            notes["empty"].as_bool()
        ),
        (Some("Bare"), None, Some(true))
    );
    write(&bare, "instructions/all/team.md", "We pair on Fridays.\n");
    assert!(kitchen_of("bare").get("empty").is_none());
    std::fs::remove_file(dir.join("repos/notes/.kito/config.toml")).unwrap();

    // The parts a repository picks are those of the kitchen it gets.
    let (ok, said) = admin(&["kitchen", "parts", "shop", "rust"]);
    assert!(
        !ok && said.contains("the kitchen has no part `rust`: it has web"),
        "{said}"
    );
    assert!(admin(&["kitchen", "parts", "shop", "web"]).0);
    assert_eq!(
        kitchen_of("shop")["concepts"],
        json!([{ "name": "page", "about": "A page is a route", "part": "web" }])
    );

    // Whoever runs the server is told which kitchen a repository gets, and why.
    let (ok, said) = admin(&["kitchen", "show", "shop"]);
    assert!(
        ok && said.contains("What `shop` gets of the kitchen Pantry: the base and web."),
        "{said}"
    );
    assert!(
        said.contains(
            "It is the repository `pantry`, which `shop` names as its kitchen (`kito-server repo set shop \
             --kitchen pantry`)."
        ),
        "{said}"
    );
    let (_, said) = admin(&["kitchen", "show", "notes"]);
    assert!(
        said.contains(
            "It is the repository `kitchen`, the server's (`kito-server kitchen set kitchen`): `notes` names \
             none of its own."
        ),
        "{said}"
    );
    let (_, said) = admin(&["kitchen", "show", "pantry"]);
    assert!(
        said.contains("`pantry` is a kitchen itself: it gets its own."),
        "{said}"
    );
    let (ok, said) = admin(&["kitchen", "show"]);
    assert!(
        ok && said.contains("Kitchen: Shopworks (the repository `kitchen`)"),
        "{said}"
    );
    assert_eq!(gets(&said, "notes"), "`kitchen` (Shopworks): the server's");
    assert_eq!(gets(&said, "pantry"), "itself (Pantry): it is a kitchen");

    // And what is wrong: a kitchen made for a repository the server does
    // not have, a repository that names a kitchen the server does not have.
    names(dir, "books", "../nowhere");
    let (ok, said) = admin(&["kitchen", "check"]);
    assert!(
        !ok && said.contains("the kitchens have 2 problems"),
        "{said}"
    );
    assert!(
        said.contains(
            "pantry/kitchen.toml: it is for `gone`, which is not a repository of this server"
        ),
        "{said}"
    );
    assert!(
        said.contains(
            "books: it names `../nowhere` as its kitchen, which is not a repository of this server"
        ),
        "{said}"
    );
    assert_eq!(gets(&said, "books"), "`kitchen` (Shopworks): the server's");
    assert_eq!(gets(&said, "shop"), "`pantry` (Pantry): its own setting");
    assert_eq!(
        gets(&said, "kitchen"),
        "itself (Shopworks): it is a kitchen"
    );

    // A repository that names one that is no kitchen is told so: it is not
    // given another instead.
    names(dir, "books", "../notes");
    let none = kitchen_of("books");
    let why = none["error"].as_str().unwrap();
    assert!(
        why.contains("`books` names the repository `notes` as its kitchen")
            && why.contains("is not a kitchen: it has no kitchen.toml"),
        "{why}"
    );
    let (ok, said) = admin(&["kitchen", "check"]);
    assert!(!ok && said.contains("books: "), "{said}");
    assert_eq!(
        gets(&said, "books"),
        "`notes` (not a kitchen): its own setting"
    );
}

/// The files of a small repository: what says how it is worked on, what
/// it tells assistants, and what is never to be read.
fn shop_files(dir: &Path) {
    write(
        dir,
        "README.md",
        "# The shop\n\nA storefront, in TypeScript.\n",
    );
    let long = format!(
        "# Contributing\n\n{}\nTHE END\n",
        "Every change needs a test. ".repeat(200)
    );
    write(dir, "CONTRIBUTING.md", &long);
    write(
        dir,
        "package.json",
        r#"{ "name": "shop", "scripts": { "test": "vitest run", "lint": "eslint .", "build": "vite build" } }"#,
    );
    write(
        dir,
        "Makefile",
        ".PHONY: all check\nall: check\ncheck:\n\tnpm run lint\n%.css: %.scss\n\tsass $< $@\n",
    );
    write(
        dir,
        ".github/workflows/ci.yml",
        "name: CI\non: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: \
         actions/checkout@v4\n      - name: Install\n        run: npm ci\n      - run: |\n          npm run \
         lint\n          npm test\n      - name: Deploy\n        run: |\n          ./configure\n          make \
         release\n          make upload\n          make announce\n",
    );
    write(dir, ".eslintrc.json", "{}\n");
    write(dir, "tsconfig.json", "{}\n");
    write(dir, "src/billing/invoice.ts", "export const total = 1;\n");
    write(dir, "tests/invoice.test.ts", "test('total', () => {});\n");
    // What it already tells assistants: its own instructions as Kito keeps
    // them, and the files assistants read, with Kito's part in one.
    write(
        dir,
        ".kito/instructions/all/style.md",
        "Two spaces, never tabs.\n",
    );
    write(
        dir,
        ".kito/instructions/claude/tests.md",
        "Run `npm test` before you say it works.\n",
    );
    write(dir, ".kito/instructions/all/taken-away.md", "\n");
    write(
        dir,
        "CLAUDE.md",
        "<!-- kito: begin (written by Kito) -->\n\n## Version control: Kito\n\nWork in a lane.\n\n<!-- kito: \
         end -->\n\nPrices are in cents.\n",
    );
    write(
        dir,
        ".github/copilot-instructions.md",
        "Prefer small functions.\n",
    );
    // What is never read, listed or included.
    write(dir, ".env", "API_KEY=hunter2\n");
    write(dir, ".env.production", "API_KEY=hunter3\n");
    write(dir, "secrets.txt", "the vault opens with swordfish\n");
    write(dir, "deploy.pem", "-----BEGIN PRIVATE KEY-----\n");
    write(
        dir,
        "node_modules/left-pad/package.json",
        r#"{ "name": "left-pad", "scripts": { "test": "node sneaky.js" } }"#,
    );
    write(dir, "target/debug/build.log", "built\n");
    write(dir, ".git/config", "[core]\n");
    write(
        dir,
        "src/secrets/keys.ts",
        "export const key = 'opensesame';\n",
    );
    write(
        dir,
        ".kito/instructions/all/tokens.md",
        "The token is xyzzy.\n",
    );
    write(dir, ".github/workflows/token-refresh.yml", "jobs: {}\n");
    std::fs::write(dir.join("README.bin"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.join("secrets.txt"), dir.join("README.link.md")).unwrap();
        std::os::unix::fs::symlink(dir.join("node_modules"), dir.join("lib")).unwrap();
    }
}

#[test]
fn a_kitchen_is_drafted_from_what_the_repositories_it_is_for_say() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    for repo in ["pantry", "shop", "books"] {
        std::fs::create_dir_all(dir.join("repos").join(repo).join(".kito")).unwrap();
    }
    let (pantry, shop) = (dir.join("repos/pantry"), dir.join("repos/shop"));
    write(
        &pantry,
        "kitchen.toml",
        "name = \"Pantry\"\nabout = \"How the shop is worked on.\"\nfor = [\"shop\", \"books\", \"gone\"]\n",
    );
    shop_files(&shop);
    names(dir, "shop", "../pantry");
    write(
        &dir.join("repos/books"),
        "README.md",
        "# The books\n\nKept by hand, mostly.\n",
    );
    let draft_as = |repo: &str, role: &str| answer_as(dir, "draft", repo, role, json!({}));

    // Only in a kitchen's own repository, and only for whoever maintains it.
    let no = draft_as("shop", "maintain");
    assert!(
        no["error"]
            .as_str()
            .unwrap()
            .contains("`shop` is not a kitchen (it has no kitchen.toml)"),
        "{no}"
    );
    for role in ["read", "write", "integrate"] {
        let no = draft_as("pantry", role);
        assert_eq!(
            no,
            json!({ "error": format!("only a maintainer of `pantry` drafts its kitchen: ada may {role} there") })
        );
    }

    let draft = draft_as("pantry", "maintain");
    assert!(draft.get("error").is_none(), "{draft}");
    // The interview, for the assistant to lead.
    let how = draft["how"].as_str().unwrap();
    assert!(
        how.starts_with("You are about to draft this team's kitchen with the person you work for"),
        "{how}"
    );
    for said in [
        "\n1. Before you ask anything, say in two or three lines what you found",
        "\n2. Ask the person how the team works, ONE question at a time",
        "\n   - what the team builds, and for whom;",
        "\n   - whether one assistant needs something the others do not.",
        "\n3. Tools: propose only commands listed under `commands`",
        "Never invent a command",
        "\n4. Write everything in ONE lane of this repository. Open it with `open_lane`",
        "about fifteen lines at most",
        "\n5. Show the person the list of the files you wrote",
        "`request_integration`",
        "\n6. When the kitchen already has content (`kitchen.has`), revise it",
    ] {
        assert!(how.contains(said), "{said}: {how}");
    }
    // The kitchen as it is: nothing in it yet.
    assert_eq!(
        draft["kitchen"],
        json!({
            "name": "Pantry",
            "about": "How the shop is worked on.",
            "for": ["shop", "books", "gone"],
            "has": { "behaviour": false, "concepts": [], "recipes": [], "tools": [], "instructions": [] },
        })
    );
    let formats = draft["formats"].as_object().unwrap();
    assert_eq!(
        formats.keys().collect::<Vec<_>>(),
        [
            "behaviour.md",
            "concepts/<name>.md",
            "instructions/<assistant>/<name>.md",
            "instructions/all/<name>.md",
            "kitchen.toml",
            "recipes/<name>.md",
            "tools/<name>.toml"
        ]
    );
    assert!(formats["tools/<name>.toml"]
        .as_str()
        .unwrap()
        .contains("`command = \"npm\"`"));

    // Of the repositories it says it is for, those that use it are read:
    // one that does not is not, and is said as one the server does not have.
    let read = draft["repositories"].as_array().unwrap();
    assert_eq!(read.len(), 1, "{draft}");
    let shop_said = &read[0];
    assert_eq!(shop_said["name"], "shop");
    assert_eq!(
        draft["left_out"],
        json!([
            "books: not a repository of this server that uses this kitchen, so nothing of it was read",
            "gone: not a repository of this server that uses this kitchen, so nothing of it was read",
        ])
    );

    // The commands it has written down, each with the file that says it:
    // none is guessed, and a long block of a workflow is a script.
    assert_eq!(
        shop_said["commands"],
        json!([
            { "name": "all", "command": "make all", "from": "Makefile" },
            { "name": "check", "command": "make check", "from": "Makefile" },
            { "name": "build", "command": "npm run build", "from": "package.json" },
            { "name": "lint", "command": "npm run lint", "from": "package.json" },
            { "name": "test", "command": "npm test", "from": "package.json" },
            { "name": "Install", "command": "npm ci", "from": ".github/workflows/ci.yml" },
            { "name": "ci", "command": "npm run lint", "from": ".github/workflows/ci.yml" },
            { "name": "ci", "command": "npm test", "from": ".github/workflows/ci.yml" },
        ])
    );
    // What says how it is worked on, a long one cut; of its lint and
    // format configuration, the names.
    let files = shop_said["files"].as_array().unwrap();
    let listed: Vec<(&str, bool)> = files
        .iter()
        .map(|file| (file["path"].as_str().unwrap(), file["cut"] == true))
        .collect();
    assert_eq!(
        listed,
        [
            ("README.md", false),
            ("CONTRIBUTING.md", true),
            ("Makefile", false),
            ("package.json", false),
            (".github/workflows/ci.yml", false),
            (".eslintrc.json", false),
            ("tsconfig.json", false),
        ]
    );
    assert_eq!(
        files[0]["text"],
        "# The shop\n\nA storefront, in TypeScript.\n"
    );
    let long = files[1]["text"].as_str().unwrap();
    assert!(
        long.chars().count() == 4000 && long.starts_with("# Contributing\n\nEvery change"),
        "{long}"
    );
    assert!(files[3]["text"].as_str().unwrap().contains("vitest run"));
    assert_eq!(
        (&files[5]["text"], &files[6]["text"]),
        (&json!(""), &json!(""))
    );
    // What it already tells assistants, without what Kito wrote.
    assert_eq!(
        shop_said["instructions"],
        json!([
            { "name": "all/style.md", "text": "Two spaces, never tabs.\n" },
            { "name": "claude/tests.md", "text": "Run `npm test` before you say it works.\n" },
            { "name": "CLAUDE.md", "text": "Prices are in cents." },
            { "name": ".github/copilot-instructions.md", "text": "Prefer small functions." },
        ])
    );
    assert_eq!(
        shop_said["layout"],
        json!([
            ".github/",
            ".github/workflows/",
            "src/",
            "src/billing/",
            "tests/",
            ".eslintrc.json",
            "CLAUDE.md",
            "CONTRIBUTING.md",
            "Makefile",
            "README.md",
            "package.json",
            "tsconfig.json",
        ])
    );
    // What is never read is nowhere in the answer: not its name, not what it holds.
    let whole = draft.to_string();
    for never in [
        ".env",
        "hunter",
        "secret",
        "swordfish",
        "deploy.pem",
        "PRIVATE KEY",
        "node_modules",
        "left-pad",
        "sneaky",
        "target/",
        "build.log",
        ".git/",
        "[core]",
        "opensesame",
        "token",
        "xyzzy",
        "README.bin",
        "README.link",
        "lib/",
        "THE END",
        "Work in a lane",
        "Kept by hand",
    ] {
        assert!(!whole.contains(never), "{never}: {whole}");
    }

    // A repository is read once it uses the kitchen, as the others are.
    names(dir, "books", "../pantry");
    let draft = draft_as("pantry", "maintain");
    assert_eq!(draft["repositories"][1]["name"], "books");
    assert_eq!(
        draft["repositories"][1]["files"],
        json!([{ "path": "README.md", "text": "# The books\n\nKept by hand, mostly.\n", "cut": false }])
    );
    assert_eq!(draft["repositories"][1]["commands"], json!([]));
    assert_eq!(draft["left_out"].as_array().unwrap().len(), 1, "{draft}");

    // What the kitchen already holds is said, for it to be revised.
    write(&pantry, "behaviour.md", "Ask before you delete.\n");
    write(
        &pantry,
        "concepts/order.md",
        "# An order is what a customer paid for\n",
    );
    write(
        &pantry,
        "recipes/release.md",
        "# How a release is made\n\n1. Tag.\n",
    );
    write(
        &pantry,
        "tools/test.toml",
        "about = \"Runs the tests\"\ncommand = \"npm\"\nargs = [\"test\"]\n",
    );
    write(&pantry, "instructions/all/team.md", "We pair on Fridays.\n");
    write(
        &pantry,
        "parts/web/concepts/page.md",
        "# A page is a route\n",
    );
    assert_eq!(
        draft_as("pantry", "maintain")["kitchen"]["has"],
        json!({
            "behaviour": true,
            "concepts": ["order"],
            "recipes": ["release"],
            "tools": ["test"],
            "instructions": ["all/team.md"],
            "parts": ["web"],
        })
    );

    // An answer stays under about 150 KB: files that would take it over
    // are not added, and which are not there is said.
    for workflow in 0..80 {
        let text = format!("name: W{workflow}\n{}", "# a long comment\n".repeat(280));
        write(
            &shop,
            &format!(".github/workflows/w{workflow:02}.yml"),
            &text,
        );
    }
    let draft = draft_as("pantry", "maintain");
    let whole = draft.to_string();
    assert!(whole.len() < 150_000, "{}", whole.len());
    let given: Vec<&str> = draft["repositories"][0]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    // What says most comes first, and is there; then the workflows, as far as there was room.
    assert_eq!(given[..2], ["README.md", "CONTRIBUTING.md"]);
    let workflows: Vec<&str> = given
        .iter()
        .copied()
        .filter(|path| path.starts_with(".github/workflows/w"))
        .collect();
    assert!(workflows.len() > 20, "{given:?}");
    let first: Vec<String> = (0..workflows.len())
        .map(|workflow| format!(".github/workflows/w{workflow:02}.yml"))
        .collect();
    assert_eq!(workflows, first);
    // The others are said, one by one as far as that goes, then counted.
    let left_out: Vec<&str> = draft["left_out"]
        .as_array()
        .unwrap()
        .iter()
        .map(|one| one.as_str().unwrap())
        .collect();
    assert_eq!(left_out.len(), 41, "{left_out:?}");
    assert_eq!(
        left_out[1],
        format!("shop: .github/workflows/w{:02}.yml", workflows.len())
    );
    let counted: usize = left_out[40]
        .strip_prefix("and ")
        .and_then(|rest| rest.strip_suffix(" more"))
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("{left_out:?}"));
    assert_eq!(workflows.len() + 39 + counted, 80, "{left_out:?}");
}

#[test]
fn outside_kito_it_says_how_it_is_run() {
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_kito-kitchen"))
            .args(args)
            .env_remove("KITO_SOCKET")
            .env_remove("KITO_PLUGIN_KEY")
            .env_remove("KITO_SERVER_DIR")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };
    let (ok, said) = run(&[]);
    assert!(
        !ok && said.contains("kito-server plugin enable kitchen --for-clients"),
        "{said}"
    );
    // An agent is answered in what it reads, not with a failure.
    let (ok, said) = run(&["tool", "kitchen"]);
    assert!(ok, "{said}");
    assert!(serde_json::from_str::<Value>(said.trim()).unwrap()["error"]
        .as_str()
        .unwrap()
        .contains("plugin of Kito"));
}

fn kito_bin() -> PathBuf {
    let dir = std::env::var_os("KITO_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../kito/target/debug"));
    let server = dir.join(format!("kito-server{}", std::env::consts::EXE_SUFFIX));
    assert!(
        server.is_file(),
        "{} is not there: this test runs kito-kitchen under Kito. Build it (`cargo build -p kito-cli -p kitod -p \
         kito-server` in the kito repository) or set KITO_BIN to where `kito`, `kitod` and `kito-server` are",
        server.display()
    );
    dir.canonicalize().unwrap()
}

#[test]
fn a_team_s_kitchen_reaches_whoever_works_in_a_connected_directory() {
    let bin = kito_bin();
    let exe = |name: &str| bin.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let (dir, theirs, mine) = (
        base.join("server"),
        base.join("server-config"),
        base.join("my-config"),
    );
    let server = |args: &[&str]| {
        let mut command = Command::new(exe("kito-server"));
        command
            .arg("--dir")
            .arg(&dir)
            .args(args)
            .env("KITOD_BIN", exe("kitod"))
            .env("KITO_AUTHOR", "the server")
            .env("KITO_CONFIG_DIR", &theirs);
        command
    };
    let ok = |args: &[&str]| {
        let out = server(args).output().unwrap();
        let (said, errors) = (
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(out.status.success(), "kito-server {args:?}: {said}{errors}");
        said.into_owned()
    };
    // `kito`, as one person on their machine, or as the server's own user.
    // The plugin runs the kitchen's tools through `kito`, found as a
    // person's shell finds it: on the PATH.
    let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let unconfined = std::cell::Cell::new(false);
    let kito = |cwd: &Path, config: &Path, args: &[&str], input: Option<&Value>| {
        let mut command = Command::new(exe("kito"));
        if unconfined.get() {
            command.env("KITO_KITCHEN_UNCONFINED", "1");
        }
        let mut child = command
            .args(args)
            .current_dir(cwd)
            .env("PATH", &path)
            .env("KITOD_BIN", exe("kitod"))
            .env("KITO_NO_CLAUDE", "1")
            .env("KITO_AUTHOR", "ada")
            .env("KITO_CONFIG_DIR", config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        if let Some(input) = input {
            stdin.write_all(input.to_string().as_bytes()).unwrap();
        }
        drop(stdin);
        let out = child.wait_with_output().unwrap();
        (
            out.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };
    let saved = |repo: &Path, message: &str| {
        let (ok, said) = kito(repo, &theirs, &["checkpoint", "-m", message], None);
        assert!(ok, "{said}");
    };

    // The plugin as it is installed from: the program and its manifest.
    let plugin = base.join("plugin");
    std::fs::create_dir_all(&plugin).unwrap();
    let program = Path::new(env!("CARGO_BIN_EXE_kito-kitchen"));
    std::fs::copy(program, plugin.join(program.file_name().unwrap())).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("kito-plugin.toml"),
        plugin.join("kito-plugin.toml"),
    )
    .unwrap();

    ok(&["init", dir.to_str().unwrap()]);
    ok(&["repo", "add", "kitchen"]);
    ok(&["repo", "add", "shop"]);
    let (kitchen, shop) = (dir.join("repos/kitchen"), dir.join("repos/shop"));
    for repo in [&kitchen, &shop] {
        write(repo, ".gitignore", ".kito/\n");
    }
    stock(&kitchen);
    saved(&kitchen, "the kitchen");
    write(&shop, "shop.ts", "export const price = 1;\n");
    saved(&shop, "first version");
    // A kitchen made for `books`, with nothing in it yet but what says so.
    ok(&["repo", "add", "pantry"]);
    ok(&["repo", "add", "books"]);
    let (pantry, books) = (dir.join("repos/pantry"), dir.join("repos/books"));
    for repo in [&pantry, &books] {
        write(repo, ".gitignore", ".kito/\n");
    }
    write(
        &pantry,
        "kitchen.toml",
        "name = \"Pantry\"\nabout = \"How the books are kept.\"\nfor = [\"books\"]\n",
    );
    saved(&pantry, "a kitchen to draft");
    write(
        &books,
        "README.md",
        "# The books\n\nWhat was sold, month by month.\n",
    );
    write(
        &books,
        "package.json",
        r#"{ "scripts": { "test": "node --test", "close": "node close-month.js" } }"#,
    );
    write(&books, ".env", "LEDGER_KEY=hunter2\n");
    saved(&books, "first version");

    ok(&["plugin", "add", "--path", plugin.to_str().unwrap()]);
    let enabled = ok(&["plugin", "enable", "kitchen", "--for-clients"]);
    assert!(
        enabled.contains("It may: read, server_admin.") && enabled.contains("kito-server kitchen"),
        "{enabled}"
    );
    assert!(ok(&["kitchen", "set", "kitchen"]).contains("Shopworks"));
    ok(&["kitchen", "parts", "shop", "rust"]);
    // `books` names its kitchen in its own configuration, which is Kito's
    // to write: the others get the server's.
    let set = ok(&["repo", "set", "books", "--kitchen", "pantry"]);
    assert!(
        set.contains("its agents get the instructions of the kitchen pantry"),
        "{set}"
    );
    let shown = ok(&["kitchen", "show", "books"]);
    assert!(
        shown.contains("What `books` gets of the kitchen Pantry: the base.")
            && shown.contains("It is the repository `pantry`, which `books` names as its kitchen"),
        "{shown}"
    );
    let checked = ok(&["kitchen", "check"]);
    assert_eq!(
        gets(&checked, "books"),
        "`pantry` (Pantry): its own setting"
    );
    assert_eq!(
        gets(&checked, "shop"),
        "`kitchen` (Shopworks): the server's"
    );
    assert!(checked.contains("The kitchens are in order."), "{checked}");
    let token_of = |added: String| {
        let token = added.lines().find(|line| line.starts_with("kito_"));
        token.unwrap().trim().to_owned()
    };
    // Ada works on the shop, reads the books, and maintains the pantry;
    // Bob writes in the pantry.
    let token = token_of(ok(&[
        "user",
        "add",
        "ada",
        "--repo",
        "shop:write",
        "--repo",
        "books:read",
        "--repo",
        "pantry:maintain",
    ]));
    let bob_s = token_of(ok(&["user", "add", "bob", "--repo", "pantry:write"]));

    struct Serving(std::process::Child);
    impl Drop for Serving {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut serving = Serving(
        server(&["serve", "--plain", "--listen", "127.0.0.1:0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut line = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(serving.0.stdout.take().unwrap()),
        &mut line,
    )
    .unwrap();
    let address = line
        .trim()
        .strip_prefix("kito-server: listening on ")
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .to_owned();

    // Ada connects a directory, and has the plugin installed.
    let desk = base.join("desk");
    let (connected, said) = kito(
        &base,
        &mine,
        &[
            "connect",
            &address,
            "shop",
            desk.to_str().unwrap(),
            "--plain",
            "--token",
            &token,
        ],
        None,
    );
    assert!(connected, "{said}");
    let (added, said) = kito(
        &desk,
        &mine,
        &["plugin", "add", "--path", plugin.to_str().unwrap()],
        None,
    );
    assert!(added, "{said}");

    // For a person.
    let (shown, said) = kito(&desk, &mine, &["kitchen"], None);
    assert!(shown, "{said}");
    assert!(
        said.starts_with(
            "The kitchen of Shopworks for `shop`, with rust\nHow we work on the shop.\n"
        ),
        "{said}"
    );
    assert!(
        said.contains("Say what you ran before you ask for an integration.")
            && said.contains("Run `cargo clippy`"),
        "{said}"
    );
    assert!(
        said.contains("Concepts (`kito kitchen concept <name>`):\n  money"),
        "{said}"
    );
    let (shown, said) = kito(&desk, &mine, &["kitchen", "concept", "money"], None);
    assert!(
        shown && said == "# Amounts are `Cents(i64)`\n\nSee `crates/money`.\n",
        "{said}"
    );
    let (shown, said) = kito(&desk, &mine, &["kitchen", "recipe", "deploy"], None);
    assert!(
        !shown && said.contains("the kitchen has no recipe `deploy`: it has release"),
        "{said}"
    );

    // For an agent: the plugin's tools, run as `kito-mcp` runs them.
    let tool = |name: &str, args: Value| -> Value {
        let (ran, said) = kito(&desk, &mine, &["kitchen", "tool", name], Some(&args));
        assert!(ran, "{said}");
        serde_json::from_str(said.trim()).unwrap_or_else(|e| panic!("{e}: {said}"))
    };
    let told = tool("kitchen", json!({}));
    assert!(
        told["read_this_first"]
            .as_str()
            .unwrap()
            .contains("It is theirs, not Kito's"),
        "{told}"
    );
    assert!(
        told["behaviour"]
            .as_str()
            .unwrap()
            .contains("Say what you ran"),
        "{told}"
    );
    assert_eq!(told["recipes"][0]["name"], "release");
    assert!(tool("recipe", json!({ "name": "release" }))["text"]
        .as_str()
        .unwrap()
        .contains("1. Tag."));
    assert!(tool("concept", json!({}))["error"]
        .as_str()
        .unwrap()
        .contains("say which concept"));
    assert!(tool("concept", json!({ "name": "nope" }))["error"]
        .as_str()
        .unwrap()
        .contains("it has money"));
    // Kito offers them to agents, with whose they are.
    let (_, listed) = kito(&desk, &mine, &["plugin", "list", "--json"], None);
    assert!(listed.contains("\"kitchen\""), "{listed}");

    // The kitchen's tools run in the working copy of the lane the agent
    // works in, with what it gave as one word, whatever is in it.
    let (opened, said) = kito(
        &desk,
        &mine,
        &[
            "lane",
            "open",
            "alice",
            "--intent",
            "a cheaper price",
            "--scope",
            "shop.ts",
        ],
        None,
    );
    assert!(opened, "{said}");
    assert!(
        desk.join(".kito/lanes/1/work/shop.ts").is_file(),
        "the lane's copy is here"
    );
    assert!(
        told["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "greet"),
        "{told}"
    );
    let ran = tool(
        "run",
        json!({ "name": "greet", "lane": "lane/1", "args": { "who": "ada; touch owned" } }),
    );
    assert_eq!(ran["exit_code"], 0, "{ran}");
    let output = ran["output"].as_str().unwrap();
    assert!(
        output.starts_with("hello ada; touch owned, from the lane's work\n")
            && output.contains("shop.ts"),
        "{output}"
    );
    assert!(!desk.join(".kito/lanes/1/work/owned").exists());
    assert!(
        tool("run", json!({ "name": "greet", "lane": "lane/1" }))["error"]
            .as_str()
            .unwrap()
            .contains("wants `who`")
    );
    assert!(tool(
        "run",
        json!({ "name": "greet", "lane": "lane/9", "args": { "who": "x" } })
    )["error"]
        .as_str()
        .unwrap()
        .contains("no working copy of the lane `lane/9`"));
    assert!(tool("run", json!({ "name": "greet" }))["error"]
        .as_str()
        .unwrap()
        .contains("the `lane` you work in"));
    let failed = tool("run", json!({ "name": "fail", "lane": "1" }));
    assert_eq!(
        (failed["exit_code"].as_i64(), failed["output"].as_str()),
        (Some(3), Some("no"))
    );
    let slow = tool("run", json!({ "name": "slow", "lane": "1" }));
    assert!(
        slow["stopped"]
            .as_str()
            .unwrap()
            .contains("stopped after 1 s")
            && slow["exit_code"].is_null(),
        "{slow}"
    );

    // A tool is confined to the lane it runs in, as a command of the lane
    // is: it writes nothing outside it and reads nothing of Kito's own.
    let out = desk.to_str().unwrap();
    let confined = tool(
        "run",
        json!({ "name": "escape", "lane": "lane/1", "args": { "out": out } }),
    );
    if confined["not_run"].is_string() {
        // A machine where Kito cannot confine a command: the tool is not run at all.
        assert!(!desk.join("escaped").exists(), "{confined}");
    } else {
        assert_eq!(confined["confined"], true, "{confined}");
        let said = confined["output"].as_str().unwrap();
        assert!(
            said.ends_with("over") && !said.contains("wrote") && !said.contains("read"),
            "{said}"
        );
        assert!(
            !desk.join("escaped").exists(),
            "a tool of the kitchen wrote outside its lane"
        );
    }
    // Unless the person says the kitchen's tools run as themselves.
    unconfined.set(true);
    let free = tool(
        "run",
        json!({ "name": "escape", "lane": "lane/1", "args": { "out": out } }),
    );
    unconfined.set(false);
    assert_eq!(free["confined"], false, "{free}");
    assert!(
        free["output"].as_str().unwrap().contains("wrote") && desk.join("escaped").is_file(),
        "{free}"
    );
    std::fs::remove_file(desk.join("escaped")).unwrap();
    // For a person, the same.
    let (ran, said) = kito(
        &desk,
        &mine,
        &["kitchen", "run", "greet", "--lane", "1", "who=bob"],
        None,
    );
    assert!(
        ran && said.starts_with("hello bob, from the lane's work"),
        "{said}"
    );
    let (ran, said) = kito(
        &desk,
        &mine,
        &["kitchen", "run", "fail", "--lane", "1"],
        None,
    );
    assert!(!ran && said.contains("ended with code 3"), "{said}");

    // When a session starts, an agent is told the kitchen after Kito's
    // own rules, without asking.
    let start = json!({ "hook_event_name": "SessionStart", "cwd": desk });
    let (started, said) = kito(&desk, &mine, &["hook", "session-start"], Some(&start));
    assert!(started, "{said}");
    let context: Value =
        serde_json::from_str(said.trim()).unwrap_or_else(|e| panic!("{e}: {said}"));
    let context = context["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    let (rules, kitchen_says) = context
        .split_once("From the plugin `kitchen`")
        .unwrap_or_else(|| panic!("{context}"));
    assert!(rules.contains("Work in a lane."), "{rules}");
    assert!(
        kitchen_says.contains(
            "The kitchen of Shopworks: how the people you work for want work done on `shop`."
        ),
        "{kitchen_says}"
    );
    assert!(
        kitchen_says.contains("Say what you ran")
            && kitchen_says.contains("- money: Amounts are `Cents(i64)`"),
        "{kitchen_says}"
    );
    assert!(
        kitchen_says.contains("- release: How a release is made")
            && kitchen_says.contains("- greet: Says hello"),
        "{kitchen_says}"
    );

    // A repository gets the kitchen its own configuration names: for who
    // reads the books, the pantry.
    let connect = |config: &Path, token: &str, repo: &str, to: &Path| {
        let to = to.to_str().unwrap();
        let args = ["connect", &address, repo, to, "--plain", "--token", token];
        let (connected, said) = kito(&base, config, &args, None);
        assert!(connected, "{said}");
    };
    let (study, counter, bench) = (base.join("study"), base.join("counter"), base.join("bench"));
    connect(&mine, &token, "books", &study);
    let (shown, said) = kito(&study, &mine, &["kitchen"], None);
    assert!(
        shown && said.starts_with("The kitchen of Pantry for `books`\nHow the books are kept.\n"),
        "{said}"
    );

    // In the pantry's own repository, where there is nothing yet, an agent
    // is told when its session starts to offer to draft it.
    connect(&mine, &token, "pantry", &counter);
    let start = json!({ "hook_event_name": "SessionStart", "cwd": counter });
    let (started, said) = kito(&counter, &mine, &["hook", "session-start"], Some(&start));
    assert!(started, "{said}");
    let context: Value =
        serde_json::from_str(said.trim()).unwrap_or_else(|e| panic!("{e}: {said}"));
    let context = context["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    let (_, pantry_says) = context
        .split_once("From the plugin `kitchen`")
        .unwrap_or_else(|| panic!("{context}"));
    assert!(
        pantry_says.contains("This repository, `pantry`, is the kitchen of Pantry")
            && pantry_says.contains("It has nothing in it yet")
            && pantry_says.contains(
                "Offer the person to draft it together: if they want to, call the kitchen's `draft` tool \
                 and follow what it says."
            ),
        "{pantry_says}"
    );
    assert!(!pantry_says.contains("Follow it."), "{pantry_says}");
    // The tool gives it the interview to lead, and what the repository
    // the pantry is for says of how it is worked on.
    let tool_in = |dir: &Path, config: &Path, name: &str| -> Value {
        let (ran, said) = kito(dir, config, &["kitchen", "tool", name], Some(&json!({})));
        assert!(ran, "{said}");
        serde_json::from_str(said.trim()).unwrap_or_else(|e| panic!("{e}: {said}"))
    };
    let draft = tool_in(&counter, &mine, "draft");
    assert!(
        draft["how"]
            .as_str()
            .is_some_and(|how| how.contains("ONE question at a time")),
        "{draft}"
    );
    assert_eq!(
        draft["kitchen"],
        json!({
            "name": "Pantry",
            "about": "How the books are kept.",
            "for": ["books"],
            "has": { "behaviour": false, "concepts": [], "recipes": [], "tools": [], "instructions": [] },
        })
    );
    let read = &draft["repositories"][0];
    assert_eq!(read["name"], "books");
    assert_eq!(
        read["commands"],
        json!([
            { "name": "close", "command": "npm run close", "from": "package.json" },
            { "name": "test", "command": "npm test", "from": "package.json" },
        ])
    );
    assert_eq!(
        read["files"][0],
        json!({ "path": "README.md", "text": "# The books\n\nWhat was sold, month by month.\n", "cut": false })
    );
    let whole = draft.to_string();
    assert!(
        !whole.contains(".env") && !whole.contains("hunter2"),
        "{whole}"
    );
    // It is asked of the server each time: no copy of it is kept.
    let copies = counter.join(".kito/plugins/kitchen/copy");
    assert!(copies.join("kitchen.json").is_file() && !copies.join("draft.json").exists());
    // It is not for an agent that works in another repository, nor for
    // somebody who does not maintain the kitchen.
    assert!(tool("draft", json!({}))["error"]
        .as_str()
        .unwrap()
        .contains("`shop` is not a kitchen"));
    let bob = base.join("bob-config");
    connect(&bob, &bob_s, "pantry", &bench);
    let (added, said) = kito(
        &bench,
        &bob,
        &["plugin", "add", "--path", plugin.to_str().unwrap()],
        None,
    );
    assert!(added, "{said}");
    assert_eq!(
        tool_in(&bench, &bob, "draft")["error"],
        "only a maintainer of `pantry` drafts its kitchen: bob may write there"
    );
    // Nor is their agent told to offer it when a session starts.
    let start = json!({ "hook_event_name": "SessionStart", "cwd": bench });
    let (started, said) = kito(&bench, &bob, &["hook", "session-start"], Some(&start));
    assert!(started, "{said}");
    assert!(
        said.contains("It has nothing in it yet")
            && said.contains("Drafting it is for a maintainer of this repository.")
            && !said.contains("Offer the person to draft it together"),
        "{said}"
    );

    // What is integrated into the kitchen is the kitchen from then on.
    write(
        &kitchen,
        "concepts/order.md",
        "# An order is what a customer paid for\n",
    );
    saved(&kitchen, "what an order is");
    let names: Vec<String> = tool("kitchen", json!({}))["concepts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|one| one["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["money", "order"]);

    // The server away, the kitchen is the one it last gave, and says so.
    drop(serving);
    let (shown, said) = kito(&desk, &mine, &["kitchen"], None);
    assert!(shown, "{said}");
    assert!(
        said.contains("could not be reached: this is the kitchen as it gave it a moment ago"),
        "{said}"
    );
    assert!(
        said.contains("Say what you ran") && said.contains("order"),
        "{said}"
    );
    let away = tool("concept", json!({ "name": "money" }));
    assert!(
        away["text"].as_str().unwrap().contains("Cents(i64)") && away["copy"].is_string(),
        "{away}"
    );
    // What was never asked for is not there to give.
    let (shown, said) = kito(&desk, &mine, &["kitchen", "concept", "order"], None);
    assert!(!shown && said.contains("the server ("), "{said}");

    for connected in [&desk, &study, &counter] {
        kito(connected, &mine, &["daemon", "stop"], None);
    }
    kito(&bench, &bob, &["daemon", "stop"], None);
    for repo in [&kitchen, &shop, &pantry, &books] {
        kito(repo, &theirs, &["daemon", "stop"], None);
    }
}
