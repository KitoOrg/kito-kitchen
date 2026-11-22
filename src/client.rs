//! In a directory connected to the server: the kitchen, asked of the
//! server through the directory's daemon (`plugin_call`, version 2 of the
//! daemon's API), for people (`kito kitchen …`) and for agents (the
//! plugin's tools, and what is said when a session starts).
//!
//! What the server answered is kept in the directory: when the server
//! cannot be reached, the kitchen is the one it last gave, and that is
//! said.
//!
//! Nothing of Kito is linked in: what this file knows is what a program
//! outside Kito is told (Kito's `docs/api.md`).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The version of the daemon's API that has `plugin_call`.
const API: u64 = 2;
/// How much of what a tool printed an agent is given: its end.
const OUTPUT_AT_MOST: usize = 20_000;
/// Seconds a tool of the kitchen may take when it does not say.
const TOOL_SECS: u64 = 120;

trait Wire: Read + Write {}
impl<T: Read + Write> Wire for T {}

fn connect(socket: &str) -> std::io::Result<Box<dyn Wire>> {
    #[cfg(unix)]
    {
        Ok(Box::new(std::os::unix::net::UnixStream::connect(socket)?))
    }
    #[cfg(windows)]
    {
        Ok(Box::new(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(socket)?,
        ))
    }
}

fn said(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// Why a question was not answered. `away`: nobody could be asked, which
/// is not the server saying no.
struct Unanswered {
    why: String,
    away: bool,
}

fn ask_server(question: &str, params: &Value) -> Result<Value, Unanswered> {
    let no = |why: String| Unanswered { why, away: false };
    let how = "kito-kitchen is a plugin of Kito: a team's server enables it (`kito-server plugin enable kitchen \
               --for-clients`), and in a directory connected to that server it is `kito kitchen`";
    let (Some(socket), Some(key)) = (said("KITO_SOCKET"), said("KITO_PLUGIN_KEY")) else {
        return Err(no(how.to_owned()));
    };
    if said("KITO_API")
        .and_then(|api| api.parse::<u64>().ok())
        .is_none_or(|api| api < API)
    {
        return Err(no(format!(
            "the kitchen needs version {API} of Kito's API (Kito 0.22): update Kito"
        )));
    }
    let away = |why: &dyn std::fmt::Display| Unanswered {
        why: format!("the daemon of this directory did not answer: {why}"),
        away: true,
    };
    let mut stream = connect(&socket).map_err(|e| away(&e))?;
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "plugin_call",
        "params": { "ask": question, "params": params },
    });
    stream
        .write_all(format!("{}\n{request}\n", json!({ "key": key })).as_bytes())
        .map_err(|e| away(&e))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| away(&e))?;
    let answer: Value =
        serde_json::from_str(&line).map_err(|_| away(&"it closed the connection"))?;
    if let Some(error) = answer.get("error") {
        let why = error["message"].as_str().unwrap_or("it refused");
        return Err(match why {
            // A repository of its own has no server, and so no kitchen.
            why if why.contains("not connected to a server") => {
                no("this repository is not connected to a server: a kitchen is a team's, kept on its server".into())
            }
            // The daemon says so when the server did not answer it.
            why if why.starts_with("the server (") => Unanswered { why: why.to_owned(), away: true },
            why => no(why.to_owned()),
        });
    }
    let answer = answer.get("result").cloned().unwrap_or(Value::Null);
    match answer["error"].as_str() {
        Some(why) => Err(no(why.to_owned())),
        None => Ok(answer),
    }
}

/// What the plugin keeps in the directory: `.kito/plugins/kitchen/`.
fn own_dir() -> Option<PathBuf> {
    Some(
        PathBuf::from(said("KITO_REPO")?)
            .join(".kito")
            .join("plugins")
            .join("kitchen"),
    )
}

fn is_name(name: &str) -> bool {
    let fits = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    !name.is_empty() && name.len() <= 64 && name.chars().all(fits) && !name.starts_with('-')
}

/// Where the copy of an answer is kept. None: no copy of it is kept.
fn copy_of(question: &str, params: &Value) -> Option<PathBuf> {
    // A draft is what other repositories say, for an interview held with
    // the server there: it is asked again each time.
    if question == "draft" {
        return None;
    }
    let name = params["name"].as_str().map(str::to_lowercase);
    let file = match name {
        Some(name) if is_name(&name) => format!("{question}-{name}.json"),
        Some(_) => return None,
        None => format!("{question}.json"),
    };
    Some(own_dir()?.join("copy").join(file))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

fn ago(ms: u64) -> String {
    let minutes = now_ms().saturating_sub(ms) / 60_000;
    let count = |n: u64, what: &str| format!("{n} {what}{} ago", if n == 1 { "" } else { "s" });
    match minutes {
        0 => "a moment ago".to_owned(),
        1..=59 => count(minutes, "minute"),
        60..=1439 => count(minutes / 60, "hour"),
        _ => count(minutes / 1440, "day"),
    }
}

/// Asks the kitchen of the server one thing. When the server cannot be
/// reached, the answer is the one it last gave, with `copy` saying so.
/// `Err` is in words for whoever reads it: a person, or an agent.
pub fn ask(question: &str, params: Value) -> Result<Value, String> {
    let copy = copy_of(question, &params);
    match ask_server(question, &params) {
        Ok(answer) => {
            // Kept for when the server is away: not kept is not an error.
            if let Some(copy) = &copy {
                let _ = std::fs::create_dir_all(copy.parent().unwrap_or(copy));
                let _ = std::fs::write(
                    copy,
                    json!({ "at_ms": now_ms(), "answer": answer }).to_string(),
                );
            }
            Ok(answer)
        }
        Err(unanswered) if unanswered.away => {
            let kept = copy.and_then(|copy| std::fs::read_to_string(copy).ok());
            let kept: Option<Value> = kept.and_then(|text| serde_json::from_str(&text).ok());
            match kept {
                Some(kept) => {
                    let mut answer = kept["answer"].clone();
                    answer["copy"] = format!(
                        "The team's server could not be reached: this is the kitchen as it gave it {}.",
                        ago(kept["at_ms"].as_u64().unwrap_or(0))
                    )
                    .into();
                    Ok(answer)
                }
                None => Err(unanswered.why),
            }
        }
        Err(unanswered) => Err(unanswered.why),
    }
}

// ----------------------------------------------------------------- tools

/// An argument of a tool as the kitchen wrote it, with `{name}` replaced
/// by what was given under that name. What was given is one word of the
/// command, whatever is in it: no shell reads it.
fn filled(arg: &str, given: &Value, tool: &str) -> Result<String, String> {
    let (mut out, mut rest) = (String::new(), arg);
    while let Some(from) = rest.find('{') {
        let Some(len) = rest[from..].find('}') else {
            break;
        };
        let key = &rest[from + 1..from + len];
        out.push_str(&rest[..from]);
        match &given[key] {
            Value::String(text) => out.push_str(text),
            Value::Number(number) => out.push_str(&number.to_string()),
            Value::Bool(flag) => out.push_str(&flag.to_string()),
            _ if !is_name(key) => out.push_str(&rest[from..=from + len]),
            _ => {
                return Err(format!(
                    "the tool `{tool}` of the kitchen wants `{key}`: give it in `args`"
                ))
            }
        }
        rest = &rest[from + len + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The working copy of a lane in this directory.
fn workdir_of(lane: &str) -> Result<PathBuf, String> {
    let id = lane.trim().trim_start_matches("lane/");
    let root = said("KITO_REPO").ok_or("the repository is not known (no KITO_REPO)")?;
    let dir = Path::new(&root)
        .join(".kito")
        .join("lanes")
        .join(id)
        .join("work");
    match is_name(&id.to_lowercase()) && dir.is_dir() {
        true => Ok(dir),
        false => Err(format!(
            "there is no working copy of the lane `{lane}` here: say the lane you work in, as `open_lane` gave it \
             (`lane/3`). A tool of the kitchen runs in a lane's working copy, never in the main directory"
        )),
    }
}

/// Stops a tool and everything it started.
fn stop(child: &mut std::process::Child) {
    // The tool is the head of a group of its own: the group goes.
    #[cfg(unix)]
    {
        let group = format!("-{}", child.id());
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &group])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs a tool of the kitchen in the working copy of a lane, and returns
/// how it ended and the end of what it printed.
///
/// It runs as a command of the lane does (`kito lane run`): confined to
/// the lane, it writes only in the working copy and reads neither Kito's
/// own data nor the person's keys, and it reaches the network only if the
/// kitchen says the tool needs it. Where Kito cannot confine a command,
/// the tool is not run, unless the person said to run the kitchen's tools
/// as themselves (`KITO_KITCHEN_UNCONFINED`).
pub fn run(name: &str, lane: &str, given: &Value) -> Result<Value, String> {
    if said("KITO_KITCHEN_NO_TOOLS").is_some() {
        return Err(
            "this machine does not run the kitchen's tools (KITO_KITCHEN_NO_TOOLS is set)"
                .to_owned(),
        );
    }
    let unconfined = said("KITO_KITCHEN_UNCONFINED").is_some();
    let workdir = workdir_of(lane)?;
    let tool = ask("tool", json!({ "name": name }))?;
    let name = tool["name"].as_str().unwrap_or(name).to_owned();
    let root =
        PathBuf::from(said("KITO_REPO").ok_or("the repository is not known (no KITO_REPO)")?);
    let own = own_dir()
        .ok_or("the repository is not known (no KITO_REPO)")?
        .join("tools")
        .join(&name);
    std::fs::create_dir_all(&own).map_err(|e| format!("{}: {e}", own.display()))?;
    let program = match tool["script"]["name"].as_str() {
        // A script the kitchen carries: written here as the server gave
        // it, in the lane's own temporary directory, which is the one
        // place outside the working copy a confined command reads.
        Some(script) => {
            let dir = match unconfined {
                true => own.clone(),
                false => workdir.parent().unwrap_or(&workdir).join("tmp"),
            };
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let file = dir.join(format!("kitchen-{name}-{script}"));
            let text = tool["script"]["text"].as_str().unwrap_or_default();
            std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
            }
            file
        }
        None => PathBuf::from(tool["command"].as_str().unwrap_or_default()),
    };
    let mut given = given.clone();
    if !given.is_object() {
        given = json!({});
    }
    given["workdir"] = workdir.to_string_lossy().into_owned().into();
    given["lane"] = lane.into();
    let mut args = Vec::new();
    for arg in tool["args"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        args.push(filled(arg, &given, &name)?);
    }
    let mut command = match unconfined {
        true => {
            let mut command = std::process::Command::new(&program);
            command.args(&args).current_dir(&workdir);
            command
        }
        // As a command of the lane is run: Kito confines it.
        false => {
            let mut command = std::process::Command::new("kito");
            command.arg("--repo").arg(&root).args(["lane", "run", lane]);
            if tool["network"] != true {
                command.arg("--no-network");
            }
            command.arg("--").arg(&program).args(&args);
            command
        }
    };
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    // What it prints goes to a file: a tool that prints much is not left
    // waiting for somebody to read, and one that is stopped leaves what
    // it had said.
    let printed = own.join("last-output");
    let out = std::fs::File::create(&printed).map_err(|e| format!("{}: {e}", printed.display()))?;
    let both = out.try_clone().map_err(|e| e.to_string())?;
    let mut child = command
        .env("KITO_LANE", lane)
        .env("KITO_KITCHEN_TOOL", &name)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(both)
        .spawn()
        .map_err(|e| match unconfined {
            true => format!("the tool `{name}` of the kitchen could not be run ({}): {e}", program.display()),
            false => format!("the tool `{name}` of the kitchen could not be run: `kito` is not on the PATH ({e})"),
        })?;
    let within = Duration::from_secs(
        tool["timeout_secs"]
            .as_u64()
            .unwrap_or(TOOL_SECS)
            .clamp(1, 3600),
    );
    let started = Instant::now();
    let (status, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (Some(status), false),
            Ok(None) if started.elapsed() < within => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                stop(&mut child);
                break (None, true);
            }
        }
    };
    let bytes = std::fs::read(&printed).unwrap_or_default();
    let end = &bytes[bytes.len().saturating_sub(OUTPUT_AT_MOST)..];
    let output = String::from_utf8_lossy(end).trim_end().to_owned();
    let mut answer = json!({
        "tool": name,
        "lane": lane,
        "exit_code": status.and_then(|status| status.code()),
        "confined": !unconfined,
        "output": output,
    });
    if timed_out {
        answer["stopped"] = format!("it was stopped after {} s", within.as_secs()).into();
    }
    if bytes.len() > OUTPUT_AT_MOST {
        answer["cut"] = "only the end of what it printed is here".into();
    }
    // Kito said it cannot confine a command on this machine: the tool did not run.
    if !unconfined && output.contains("a command cannot be confined here") {
        answer["not_run"] = "Kito cannot confine a command on this machine, so the kitchen's tool was not run. The \
                             person may run the kitchen's tools as themselves, unconfined: KITO_KITCHEN_UNCONFINED=1."
            .into();
    }
    Ok(answer)
}

// ------------------------------------------------------------ for agents

/// What the answer to a tool begins with: whose words these are.
const WHOSE: &str = "This is the team's kitchen: how the people you work for want work done on their repositories. \
                     Follow it. It is theirs, not Kito's: Kito's own rules (lanes, scope, why) stand whatever it says.";

/// `kito-kitchen tool <name>`: for an agent, the arguments as JSON on the
/// input and the answer as JSON on the output. A question that cannot be
/// answered is answered with why, in `error`: the agent reads it.
pub fn tool(name: &str) -> Value {
    let args: Value = serde_json::from_reader(std::io::stdin()).unwrap_or(Value::Null);
    let answered = match name {
        "kitchen" => ask("kitchen", json!({})).map(|mut kitchen| {
            kitchen["read_this_first"] = WHOSE.into();
            kitchen["more"] = "`concept` and `recipe` give the text of one, by its name; `run` runs one of the \
                               tools, in the lane you work in."
                .into();
            kitchen
        }),
        "concept" | "recipe" => match args["name"].as_str() {
            Some(asked) => ask(name, json!({ "name": asked })),
            None => Err(format!("say which {name}: its `name`, as the tool `kitchen` lists them")),
        },
        "run" => match (args["name"].as_str(), args["lane"].as_str()) {
            (Some(tool), Some(lane)) => run(tool, lane, &args["args"]),
            _ => Err("say which tool (`name`, as the tool `kitchen` lists them) and the `lane` you work in".to_owned()),
        },
        // The interview to draft the kitchen by, and what it is drafted
        // from: the server says both, and to whom.
        "draft" => ask("draft", json!({})),
        other => Err(format!("the kitchen has no tool `{other}`")),
    };
    answered.unwrap_or_else(|why| json!({ "error": why }))
}

fn lines(entries: &Value) -> String {
    let entries = entries.as_array().map(Vec::as_slice).unwrap_or_default();
    let line = |entry: &Value| {
        format!(
            "- {}: {}\n",
            entry["name"].as_str().unwrap_or_default(),
            entry["about"].as_str().unwrap_or_default()
        )
    };
    entries.iter().map(line).collect()
}

/// `kito-kitchen session`: what an agent is told when a session starts,
/// after Kito's own rules: the kitchen, or, in the repository of a kitchen
/// that is still to be written, to offer to draft it. Nothing, when there
/// is no kitchen to tell.
pub fn session() -> String {
    let Ok(kitchen) = ask("kitchen", json!({})) else {
        return String::new();
    };
    let text = |key: &str| kitchen[key].as_str().unwrap_or_default();
    let mut told = match kitchen["own"] == true && kitchen["empty"] == true {
        // In the kitchen's own repository, with nothing in it yet: there
        // is nothing to follow, and for whoever maintains it, somebody to
        // write it with.
        true => format!(
            "This repository, `{}`, is the kitchen of {}: what every agent of the team is told to work on its \
             repositories. It has nothing in it yet: no behaviour, no concept, no recipe, no tool, no \
             instruction. {}\n",
            text("repository"),
            text("kitchen"),
            match kitchen["drafts"] == true {
                true => {
                    "Offer the person to draft it together: if they want to, call the kitchen's `draft` tool \
                     and follow what it says."
                }
                false => "Drafting it is for a maintainer of this repository.",
            }
        ),
        false => format!(
            "The kitchen of {}: how the people you work for want work done on `{}`. Follow it.\n",
            text("kitchen"),
            text("repository")
        ),
    };
    if !text("copy").is_empty() {
        told.push_str(&format!("{}\n", text("copy")));
    }
    if !text("behaviour").is_empty() {
        told.push_str(&format!("\n{}\n", text("behaviour")));
    }
    let lists = [
        ("concepts", "Concepts, what the team's words mean (the kitchen's `concept` tool gives the text of one):"),
        ("recipes", "Recipes, how things are done here (the kitchen's `recipe` tool: follow one before doing what it is for):"),
        ("tools", "Tools every agent of the team has (the kitchen's `run` tool, with `name`, `lane` and `args`):"),
    ];
    for (key, title) in lists {
        let listed = lines(&kitchen[key]);
        if !listed.is_empty() {
            told.push_str(&format!("\n{title}\n{listed}"));
        }
    }
    told
}

// ------------------------------------------------------------ for people

fn list(title: &str, how: &str, entries: &Value) {
    let entries = entries.as_array().map(Vec::as_slice).unwrap_or_default();
    if entries.is_empty() {
        return;
    }
    println!("\n{title} (`kito kitchen {how}`):");
    for entry in entries {
        let text = |key: &str| entry[key].as_str().unwrap_or_default();
        let from = entry["part"]
            .as_str()
            .map(|part| format!("  [{part}]"))
            .unwrap_or_default();
        println!("  {:<20} {}{from}", text("name"), text("about"));
    }
}

/// `kito kitchen [concept | recipe <name>]`, `kito kitchen run <tool>
/// --lane <lane> [name=value…]`: for a person.
pub fn show(args: &[&str]) -> Result<(), String> {
    match args {
        [] => {
            let kitchen = ask("kitchen", json!({}))?;
            let text = |key: &str| kitchen[key].as_str().unwrap_or_default();
            let parts: Vec<&str> = kitchen["parts"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let parts = if parts.is_empty() { String::new() } else { format!(", with {}", parts.join(", ")) };
            println!("The kitchen of {} for `{}`{parts}", text("kitchen"), text("repository"));
            if !text("copy").is_empty() {
                println!("{}", text("copy"));
            }
            if !text("about").is_empty() {
                println!("{}", text("about"));
            }
            match text("behaviour") {
                "" => println!("\n(no behaviour written yet: `behaviour.md` in the kitchen)"),
                behaviour => println!("\n{behaviour}"),
            }
            list("Concepts", "concept <name>", &kitchen["concepts"]);
            list("Recipes", "recipe <name>", &kitchen["recipes"]);
            list("Tools", "run <name> --lane <lane> [name=value…]", &kitchen["tools"]);
            Ok(())
        }
        [kind @ ("concept" | "recipe"), name] => {
            let one = ask(kind, json!({ "name": name }))?;
            if let Some(copy) = one["copy"].as_str() {
                eprintln!("{copy}");
            }
            print!("{}", one["text"].as_str().unwrap_or_default());
            Ok(())
        }
        ["run", name, rest @ ..] => {
            let (mut lane, mut given) = (None, serde_json::Map::new());
            let mut rest = rest.iter();
            while let Some(word) = rest.next() {
                match (*word, word.split_once('=')) {
                    ("--lane", _) => lane = rest.next().copied(),
                    (_, Some((key, value))) => drop(given.insert(key.to_owned(), value.into())),
                    _ => return Err(format!("`{word}`? What a tool is given is written name=value")),
                }
            }
            let lane = lane.ok_or("say the lane it runs in: `--lane 3`")?;
            let ran = run(name, lane, &Value::Object(given))?;
            println!("{}", ran["output"].as_str().unwrap_or_default());
            match (ran["stopped"].as_str(), ran["exit_code"].as_i64()) {
                (Some(stopped), _) => Err(format!("the tool `{name}`: {stopped}")),
                (None, Some(0)) => Ok(()),
                (None, code) => Err(format!("the tool `{name}` ended with {}", code.map_or("a signal".to_owned(), |code| format!("code {code}")))),
            }
        }
        _ => Err("it is `kito kitchen`, `kito kitchen concept <name>`, `kito kitchen recipe <name>` or `kito kitchen \
                  run <tool> --lane <lane> [name=value…]`"
            .to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_tool_is_given_is_one_word_whatever_is_in_it() {
        let given = json!({ "filter": "total; rm -rf /", "n": 3, "all": true });
        assert_eq!(
            filled("--grep={filter}", &given, "test").unwrap(),
            "--grep=total; rm -rf /"
        );
        assert_eq!(filled("{n}/{all}", &given, "test").unwrap(), "3/true");
        assert_eq!(filled("plain", &given, "test").unwrap(), "plain");
        // What is no name is no place to fill: braces of the command's own.
        assert_eq!(
            filled("{ \"a\": 1 }", &given, "test").unwrap(),
            "{ \"a\": 1 }"
        );
        assert!(filled("{missing}", &given, "test")
            .unwrap_err()
            .contains("wants `missing`"));
    }
}
