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
}

/// Runs the plugin as a server runs it: its directory in the environment.
fn on_server(
    dir: &Path,
    args: &[&str],
    asked_repo: Option<&str>,
    input: Option<&Value>,
) -> (bool, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kito-kitchen"));
    command
        .args(args)
        .env("KITO_SERVER_DIR", dir)
        .env_remove("KITO_SOCKET")
        .env_remove("KITO_PLUGIN_KEY");
    if let Some(repo) = asked_repo {
        command
            .env("KITO_ASKED_REPO", repo)
            .env("KITO_ASKED_BY", "ada")
            .env("KITO_ASKED_ROLE", "write");
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

fn answer(dir: &Path, question: &str, repo: &str, params: Value) -> Value {
    let (ok, said) = on_server(dir, &["answer", question], Some(repo), Some(&params));
    assert!(ok, "{said}");
    serde_json::from_str(said.lines().last().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {said}"))
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
    assert!(ok && said.contains("5 files"), "{said}");
    assert!(std::fs::read_to_string(new.join("kitchen.toml"))
        .unwrap()
        .contains("name = \"Our team\""));
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
    let kito = |cwd: &Path, config: &Path, args: &[&str], input: Option<&Value>| {
        let mut child = Command::new(exe("kito"))
            .args(args)
            .current_dir(cwd)
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

    ok(&["plugin", "add", "--path", plugin.to_str().unwrap()]);
    let enabled = ok(&["plugin", "enable", "kitchen", "--for-clients"]);
    assert!(
        enabled.contains("It may: read, server_admin.") && enabled.contains("kito-server kitchen"),
        "{enabled}"
    );
    assert!(ok(&["kitchen", "set", "kitchen"]).contains("Shopworks"));
    ok(&["kitchen", "parts", "shop", "rust"]);
    let token = ok(&["user", "add", "ada", "--role", "write"]);
    let token = token
        .lines()
        .find(|line| line.starts_with("kito_"))
        .unwrap()
        .trim()
        .to_owned();

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

    kito(&desk, &mine, &["daemon", "stop"], None);
    for repo in [&kitchen, &shop] {
        kito(repo, &theirs, &["daemon", "stop"], None);
    }
}
