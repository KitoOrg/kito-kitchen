//! In a directory connected to the server: the kitchen, asked of the
//! server through the directory's daemon (`plugin_call`, version 2 of the
//! daemon's API), for people (`kito kitchen …`) and for agents (the
//! plugin's tools).
//!
//! Nothing of Kito is linked in: what this file knows is what a program
//! outside Kito is told (Kito's `docs/api.md`).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};

/// The version of the daemon's API that has `plugin_call`.
const API: u64 = 2;

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

/// Asks the kitchen of the server one thing. `Err` is in words for
/// whoever reads it: a person, or an agent.
pub fn ask(question: &str, params: Value) -> Result<Value, String> {
    let said = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let how = "kito-kitchen is a plugin of Kito: a team's server enables it (`kito-server plugin enable kitchen \
               --for-clients`), and in a directory connected to that server it is `kito kitchen`";
    let (Some(socket), Some(key)) = (said("KITO_SOCKET"), said("KITO_PLUGIN_KEY")) else {
        return Err(how.to_owned());
    };
    if said("KITO_API")
        .and_then(|api| api.parse::<u64>().ok())
        .is_none_or(|api| api < API)
    {
        return Err(format!(
            "the kitchen needs version {API} of Kito's API (Kito 0.22): update Kito"
        ));
    }
    let unreachable =
        |why: &dyn std::fmt::Display| format!("the daemon of this directory did not answer: {why}");
    let mut stream = connect(&socket).map_err(|e| unreachable(&e))?;
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "plugin_call",
        "params": { "ask": question, "params": params },
    });
    stream
        .write_all(format!("{}\n{request}\n", json!({ "key": key })).as_bytes())
        .map_err(|e| unreachable(&e))?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| unreachable(&e))?;
    let answer: Value =
        serde_json::from_str(&line).map_err(|_| unreachable(&"it closed the connection"))?;
    if let Some(error) = answer.get("error") {
        let why = error["message"].as_str().unwrap_or("it refused");
        // A repository of its own has no server, and so no kitchen.
        return Err(match why.contains("not connected to a server") {
            true => "this repository is not connected to a server: a kitchen is a team's, kept on its server".to_owned(),
            false => why.to_owned(),
        });
    }
    let answer = answer.get("result").cloned().unwrap_or(Value::Null);
    match answer["error"].as_str() {
        Some(why) => Err(why.to_owned()),
        None => Ok(answer),
    }
}

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
            kitchen["more"] = "`concept` and `recipe` give the text of one, by its name.".into();
            kitchen
        }),
        "concept" | "recipe" => match args["name"].as_str() {
            Some(asked) => ask(name, json!({ "name": asked })),
            None => Err(format!(
                "say which {name}: its `name`, as the tool `kitchen` lists them"
            )),
        },
        other => Err(format!("the kitchen has no tool `{other}`")),
    };
    answered.unwrap_or_else(|why| json!({ "error": why }))
}

fn list(title: &str, how: &str, entries: &Value) {
    let entries = entries.as_array().map(Vec::as_slice).unwrap_or_default();
    if entries.is_empty() {
        return;
    }
    println!("\n{title} (`kito kitchen {how} <name>`):");
    for entry in entries {
        let text = |key: &str| entry[key].as_str().unwrap_or_default();
        let from = entry["part"]
            .as_str()
            .map(|part| format!("  [{part}]"))
            .unwrap_or_default();
        println!("  {:<20} {}{from}", text("name"), text("about"));
    }
}

/// `kito kitchen [concept | recipe <name>]`: for a person.
pub fn show(args: &[&str]) -> Result<(), String> {
    match args {
        [] => {
            let kitchen = ask("kitchen", json!({}))?;
            let text = |key: &str| kitchen[key].as_str().unwrap_or_default();
            let parts: Vec<&str> = kitchen["parts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let parts = if parts.is_empty() {
                String::new()
            } else {
                format!(", with {}", parts.join(", "))
            };
            println!(
                "The kitchen of {} for `{}`{parts}",
                text("kitchen"),
                text("repository")
            );
            if !text("about").is_empty() {
                println!("{}", text("about"));
            }
            match text("behaviour") {
                "" => println!("\n(no behaviour written yet: `behaviour.md` in the kitchen)"),
                behaviour => println!("\n{behaviour}"),
            }
            list("Concepts", "concept", &kitchen["concepts"]);
            list("Recipes", "recipe", &kitchen["recipes"]);
            Ok(())
        }
        [kind @ ("concept" | "recipe"), name] => {
            let one = ask(kind, json!({ "name": name }))?;
            print!("{}", one["text"].as_str().unwrap_or_default());
            Ok(())
        }
        _ => Err(
            "it is `kito kitchen`, `kito kitchen concept <name>` or `kito kitchen recipe <name>`"
                .to_owned(),
        ),
    }
}
