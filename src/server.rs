//! On the server: which repository is the kitchen, which parts each
//! repository picked, and what is answered to the directories connected
//! to it.

use crate::kitchen::{self, Kitchen};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The server's directory, and who asks from a connected directory, as
/// Kito says them.
const ENV_SERVER_DIR: &str = "KITO_SERVER_DIR";
const ENV_ASKED_REPO: &str = "KITO_ASKED_REPO";

/// What the plugin keeps on a server: `plugins/kitchen/config.toml`.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    /// The repository of the server that is the kitchen.
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<String>,
    /// The parts each repository picked, on top of the base.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    parts: BTreeMap<String, Vec<String>>,
}

struct Server {
    dir: PathBuf,
    config: Config,
}

impl Server {
    fn open() -> Result<Server, String> {
        let dir = std::env::var_os(ENV_SERVER_DIR)
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .ok_or("KITO_SERVER_DIR is not set: this is run by a Kito server (`kito-server kitchen …`)")?;
        let path = dir.join("plugins/kitchen/config.toml");
        let config = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?,
            Err(_) => Config::default(),
        };
        Ok(Server { dir, config })
    }

    fn save(&self) -> Result<(), String> {
        let path = self.dir.join("plugins/kitchen/config.toml");
        let said = |e: std::io::Error| format!("{}: {e}", path.display());
        std::fs::create_dir_all(path.parent().unwrap_or(&path)).map_err(said)?;
        let text = toml::to_string(&self.config).map_err(|e| e.to_string())?;
        let staged = path.with_extension("toml.new");
        std::fs::write(&staged, text)
            .and_then(|()| std::fs::rename(&staged, &path))
            .map_err(said)
    }

    /// Where a repository of the server is: its main directory is its main
    /// lane, what was integrated.
    fn repository(&self, name: &str) -> Result<PathBuf, String> {
        let dir = self.dir.join("repos").join(name);
        // A name, and not a way out of where the repositories are.
        let plain = !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\']);
        match plain && dir.join(".kito").is_dir() {
            true => Ok(dir),
            false => Err(format!(
                "this server has no repository `{name}` (`kito-server repo list`)"
            )),
        }
    }

    /// The kitchen's directory. `Err`: the server has none, and how to say which it is.
    fn kitchen_dir(&self) -> Result<PathBuf, String> {
        let name = self.config.repository.as_deref().ok_or(
            "this server has no kitchen: `kito-server kitchen set <repository>` says which repository it is",
        )?;
        self.repository(name)
    }

    /// What a repository gets of the kitchen.
    fn kitchen_of(&self, repo: &str) -> Result<Kitchen, String> {
        let picked = self.config.parts.get(repo).cloned().unwrap_or_default();
        kitchen::load(&self.kitchen_dir()?, &picked)
    }
}

fn listed(entries: &BTreeMap<String, kitchen::Entry>) -> Vec<Value> {
    entries.values().map(|entry| json!(entry)).collect()
}

/// `kito-kitchen answer <question>`: what the same plugin in a connected
/// directory asked, answered for the repository it is connected to. What
/// cannot be answered is said in the answer (`error`), which whoever asked
/// reads: the plugin did not fail.
pub fn answer(question: &str, params: &Value) -> Result<Value, String> {
    let server = Server::open()?;
    let repo = std::env::var(ENV_ASKED_REPO).unwrap_or_default();
    let kitchen = match server.kitchen_of(&repo) {
        Ok(kitchen) => kitchen,
        Err(why) => return Ok(json!({ "error": why })),
    };
    let one = |kind: &str, entries: &BTreeMap<String, kitchen::Entry>| {
        let name = params["name"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        match entries.get(&name).filter(|_| kitchen::is_name(&name)) {
            Some(entry) => match std::fs::read_to_string(&entry.file) {
                Ok(text) => {
                    json!({ "name": entry.name, "about": entry.about, "part": entry.part, "text": text })
                }
                Err(e) => json!({ "error": format!("the {kind} `{name}` could not be read: {e}") }),
            },
            None => {
                let known: Vec<&str> = entries.keys().map(String::as_str).collect();
                let known = if known.is_empty() {
                    "it has none".to_owned()
                } else {
                    format!("it has {}", known.join(", "))
                };
                json!({ "error": format!("the kitchen has no {kind} `{name}`: {known}") })
            }
        }
    };
    Ok(match question {
        "kitchen" => json!({
            "kitchen": kitchen.name,
            "about": kitchen.about,
            "repository": repo,
            "parts": kitchen.parts,
            "behaviour": kitchen.behaviour,
            "concepts": listed(&kitchen.concepts),
            "recipes": listed(&kitchen.recipes),
        }),
        "concept" => one("concept", &kitchen.concepts),
        "recipe" => one("recipe", &kitchen.recipes),
        other => {
            json!({ "error": format!("the kitchen of this server does not know the question `{other}`: update kito-kitchen there") })
        }
    })
}

fn show(kitchen: &Kitchen) {
    for (title, entries) in [
        ("Concepts", &kitchen.concepts),
        ("Recipes", &kitchen.recipes),
    ] {
        println!("{title}: {}", if entries.is_empty() { "none" } else { "" });
        for entry in entries.values() {
            let from = entry
                .part
                .as_deref()
                .map(|part| format!("  [{part}]"))
                .unwrap_or_default();
            println!("  {:<20} {}{from}", entry.name, entry.about);
        }
    }
}

/// `kito-server kitchen …`: for whoever runs the server.
pub fn admin(args: &[String]) -> Result<(), String> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        // A kitchen to start from, anywhere: no server needed.
        ["new", dir, name @ ..] => {
            let name = if name.is_empty() { "Our team".to_owned() } else { name.join(" ") };
            let written = kitchen::start(Path::new(dir), &name)?;
            println!("A kitchen to start from is in {dir} ({} files).", written.len());
            println!("Make it a repository of the server (`kito-server repo add kitchen --from <a git repository of it>`),");
            println!("then `kito-server kitchen set kitchen`.");
            Ok(())
        }
        ["set", repo] => {
            let mut server = Server::open()?;
            let dir = server.repository(repo)?;
            let kitchen = kitchen::load(&dir, &[])?;
            server.config.repository = Some((*repo).to_owned());
            server.save()?;
            println!("The kitchen of this server is the repository `{repo}`: {}.", kitchen.name);
            println!("{} and {}, {} to pick.", count(kitchen.concepts.len(), "concept"), count(kitchen.recipes.len(), "recipe"), count(kitchen::parts_of(&dir).len(), "part"));
            println!("It is changed through lanes in that repository, and is what its main lane has.");
            Ok(())
        }
        ["parts", repo, picked @ ..] => {
            let mut server = Server::open()?;
            server.repository(repo)?;
            let there = kitchen::parts_of(&server.kitchen_dir()?);
            if let Some(missing) = picked.iter().find(|part| !there.iter().any(|known| known == *part)) {
                let known = if there.is_empty() { "it has none".to_owned() } else { format!("it has {}", there.join(", ")) };
                return Err(format!("the kitchen has no part `{missing}`: {known}"));
            }
            match picked.is_empty() {
                true => drop(server.config.parts.remove(*repo)),
                false => drop(server.config.parts.insert((*repo).to_owned(), picked.iter().map(|part| (*part).to_owned()).collect())),
            }
            server.save()?;
            match picked.is_empty() {
                true => println!("`{repo}` gets the kitchen's base, and no part."),
                false => println!("`{repo}` gets the kitchen's base and: {}.", picked.join(", ")),
            }
            Ok(())
        }
        ["show"] | [] => {
            let server = Server::open()?;
            let dir = server.kitchen_dir()?;
            let kitchen = kitchen::load(&dir, &[])?;
            println!("Kitchen: {} (the repository `{}`)", kitchen.name, server.config.repository.as_deref().unwrap_or_default());
            let parts = kitchen::parts_of(&dir);
            println!("Parts: {}", if parts.is_empty() { "none".to_owned() } else { parts.join(", ") });
            for (repo, picked) in &server.config.parts {
                println!("  {repo} picked {}", picked.join(", "));
            }
            show(&kitchen);
            Ok(())
        }
        ["show", repo] => {
            let server = Server::open()?;
            server.repository(repo)?;
            let kitchen = server.kitchen_of(repo)?;
            let parts = if kitchen.parts.is_empty() { "the base".to_owned() } else { format!("the base and {}", kitchen.parts.join(", ")) };
            println!("What `{repo}` gets of the kitchen {}: {parts}.", kitchen.name);
            println!("\n{}\n", if kitchen.behaviour.is_empty() { "(no behaviour written)" } else { &kitchen.behaviour });
            show(&kitchen);
            Ok(())
        }
        ["check"] => {
            let server = Server::open()?;
            let dir = server.kitchen_dir()?;
            // Everything there is, and what each repository picked.
            let mut problems = kitchen::load(&dir, &kitchen::parts_of(&dir))?.problems;
            for (repo, picked) in &server.config.parts {
                let wrong = kitchen::load(&dir, picked)?.problems;
                problems.extend(wrong.into_iter().filter(|problem| problem.starts_with("the part")).map(|problem| format!("{repo}: {problem}")));
            }
            problems.dedup();
            match problems.is_empty() {
                true => {
                    println!("The kitchen is in order.");
                    Ok(())
                }
                false => Err(format!("the kitchen has {}:\n  {}", count(problems.len(), "problem"), problems.join("\n  "))),
            }
        }
        _ => Err("it is `kito-server kitchen new <directory> [name] | set <repository> | parts <repository> [part…] | \
                  show [repository] | check`"
            .to_owned()),
    }
}

fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}
