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

    ok(&["plugin", "add", "--path", plugin.to_str().unwrap()]);
    let enabled = ok(&["plugin", "enable", "kitchen", "--for-clients"]);
    assert!(
        enabled.contains("It may: read, server_admin.") && enabled.contains("kito-server kitchen"),
        "{enabled}"
    );
    assert!(ok(&["kitchen", "set", "kitchen"]).contains("Shopworks"));
    ok(&["kitchen", "parts", "shop", "rust"]);
    let token = ok(&["user", "add", "ada", "--repo", "shop:write"]);
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

    kito(&desk, &mine, &["daemon", "stop"], None);
    for repo in [&kitchen, &shop] {
        kito(repo, &theirs, &["daemon", "stop"], None);
    }
}
