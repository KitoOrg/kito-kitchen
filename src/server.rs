//! On the server: which kitchen each repository gets, which parts it
//! picked, and what is answered to the directories connected to it.

use crate::draft;
use crate::kitchen::{self, Kitchen};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The server's directory, and who asks from a connected directory (their
/// name, the repository, what they may do in it), as Kito says them.
const ENV_SERVER_DIR: &str = "KITO_SERVER_DIR";
const ENV_ASKED_BY: &str = "KITO_ASKED_BY";
const ENV_ASKED_REPO: &str = "KITO_ASKED_REPO";
const ENV_ASKED_ROLE: &str = "KITO_ASKED_ROLE";
/// The role of whoever changes a repository's rules, which the server's
/// admins have in every repository: who drafts a kitchen.
const MAINTAIN: &str = "maintain";

/// What the plugin keeps on a server: `plugins/kitchen/config.toml`.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    /// The repository of the server that is the kitchen of every
    /// repository that names none of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<String>,
    /// The parts each repository picked, on top of the base.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    parts: BTreeMap<String, Vec<String>>,
}

/// What is read of a repository's own configuration (`.kito/config.toml`,
/// which is Kito's): the kitchen it names, as `kito-server repo set
/// <repository> --kitchen` and the web client's settings write it.
#[derive(Default, Deserialize)]
#[serde(default)]
struct Settings {
    kitchen: Named,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Named {
    /// A directory, from the repository when it is not absolute: `../kitchen`.
    path: Option<String>,
}

/// Why a repository gets the kitchen it gets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Why {
    /// It is a kitchen itself.
    Itself,
    /// Its own configuration names it.
    Named,
    /// It is the server's, for the repositories that name none.
    Servers,
}

/// The kitchen a repository gets: the repository of the server it is,
/// where that is, and why it is that one.
struct Gets {
    kitchen: String,
    dir: PathBuf,
    why: Why,
}

impl Gets {
    /// Which it is and why, for whoever runs the server.
    fn said(&self, repo: &str) -> String {
        let kitchen = &self.kitchen;
        match self.why {
            Why::Itself => format!("`{repo}` is a kitchen itself: it gets its own."),
            Why::Named => format!(
                "It is the repository `{kitchen}`, which `{repo}` names as its kitchen (`kito-server repo set \
                 {repo} --kitchen {kitchen}`)."
            ),
            Why::Servers => format!(
                "It is the repository `{kitchen}`, the server's (`kito-server kitchen set {kitchen}`): `{repo}` \
                 names none of its own."
            ),
        }
    }
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

    /// The repositories of the server, by name.
    fn repositories(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.dir.join("repos"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| self.repository(name).is_ok())
            .collect();
        names.sort();
        names
    }

    /// What the configuration of the repository in `dir` names as its
    /// kitchen, as it is written there. None: it names none.
    fn named_in(dir: &Path) -> Option<String> {
        let text = std::fs::read_to_string(dir.join(".kito").join("config.toml")).ok()?;
        let settings: Settings = toml::from_str(&text).ok()?;
        let path = settings.kitchen.path?.trim().to_owned();
        (!path.is_empty()).then_some(path)
    }

    /// The repository of this server that `path` is, written in the
    /// configuration of the repository in `dir`. None: it is none of them.
    fn repository_at(&self, dir: &Path, path: &str) -> Option<String> {
        let named = match Path::new(path).is_absolute() {
            true => PathBuf::from(path),
            false => dir.join(path),
        };
        // The directory it is, whatever way the path takes to it.
        let named = named.canonicalize().ok()?;
        let repos = self.dir.join("repos").canonicalize().ok()?;
        let name = named.strip_prefix(repos).ok()?.to_str()?;
        self.repository(name).ok().map(|_| name.to_owned())
    }

    /// The kitchen of the whole server: where it is. `Err`: the server has
    /// none, and how to say which it is.
    fn kitchen_dir(&self) -> Result<PathBuf, String> {
        let name = self.config.repository.as_deref().ok_or(
            "this server has no kitchen: `kito-server kitchen set <repository>` says which repository it is",
        )?;
        self.repository(name)
    }

    /// Which kitchen a repository gets: itself, when it is one; else the
    /// one its own configuration names, when that is a repository of this
    /// server; else the server's.
    fn gets(&self, repo: &str) -> Result<Gets, String> {
        if let Ok(dir) = self.repository(repo) {
            // Who works on a kitchen sees that kitchen.
            if dir.join("kitchen.toml").is_file() {
                let (kitchen, why) = (repo.to_owned(), Why::Itself);
                return Ok(Gets { kitchen, dir, why });
            }
            let named = Self::named_in(&dir).and_then(|path| self.repository_at(&dir, &path));
            if let Some(kitchen) = named {
                let (dir, why) = (self.repository(&kitchen)?, Why::Named);
                return Ok(Gets { kitchen, dir, why });
            }
        }
        let dir = self.kitchen_dir()?;
        let kitchen = self.config.repository.clone().unwrap_or_default();
        Ok(Gets {
            kitchen,
            dir,
            why: Why::Servers,
        })
    }

    /// Which kitchen each repository of the server gets; none: it gets none.
    fn who_gets_what(&self) -> Vec<(String, Option<Gets>)> {
        let repos = self.repositories().into_iter();
        repos
            .map(|repo| {
                let gets = self.gets(&repo).ok();
                (repo, gets)
            })
            .collect()
    }

    /// What a repository gets of its kitchen, and which kitchen that is.
    fn kitchen_of(&self, repo: &str) -> Result<(Gets, Kitchen), String> {
        let gets = self.gets(repo)?;
        let picked = self.config.parts.get(repo).cloned().unwrap_or_default();
        let kitchen = kitchen::load(&gets.dir, &picked).map_err(|why| match gets.why {
            Why::Named => format!(
                "`{repo}` names the repository `{}` as its kitchen: {why}",
                gets.kitchen
            ),
            _ => why,
        })?;
        Ok((gets, kitchen))
    }

    /// What the tool `draft` is answered in the repository `repo`: only in
    /// a kitchen's own repository, and only to whoever maintains it.
    fn draft(&self, repo: &str) -> Result<Value, String> {
        let dir = match self.gets(repo) {
            Ok(Gets {
                why: Why::Itself,
                dir,
                ..
            }) => dir,
            _ => {
                return Err(format!(
                    "`{repo}` is not a kitchen (it has no kitchen.toml): a kitchen is drafted in its own \
                     repository, from a directory connected to that one (`kito connect`)"
                ))
            }
        };
        let said = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        let role = said(ENV_ASKED_ROLE);
        if role.as_deref() != Some(MAINTAIN) {
            return Err(format!(
                "only a maintainer of `{repo}` drafts its kitchen: {} may {} there",
                said(ENV_ASKED_BY).unwrap_or_else(|| "whoever asks".to_owned()),
                role.unwrap_or_else(|| "do nothing".to_owned())
            ));
        }
        let kitchen = kitchen::load(&dir, &[])?;
        // Of the repositories it says it is for, those that get it are
        // read: whoever changes a kitchen is trusted by the repositories
        // that use it, and by no other.
        let (mut read, mut not_read) = (Vec::new(), Vec::new());
        for name in &kitchen.made_for {
            let uses = self.gets(name).is_ok_and(|theirs| theirs.kitchen == repo);
            match self.repository(name) {
                Ok(dir) if uses => read.push((name.clone(), dir)),
                _ => not_read.push(name.clone()),
            }
        }
        Ok(draft::answer(&dir, &kitchen, &read, &not_read))
    }
}

/// What a tool takes, as a JSON Schema: what the kitchen wrote, or anything.
fn input_of(tool: &kitchen::Tool) -> Value {
    let written = tool
        .spec
        .input
        .as_ref()
        .and_then(|input| serde_json::to_value(input).ok());
    written
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({ "type": "object" }))
}

fn tool_listed(tool: &kitchen::Tool) -> Value {
    json!({ "name": tool.name, "about": tool.spec.about.trim(), "part": tool.part, "input": input_of(tool) })
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
    // A draft is of the kitchen whose repository this is, not of the one it gets.
    if question == "draft" {
        let drafted = server.draft(&repo);
        return Ok(drafted.unwrap_or_else(|why| json!({ "error": why })));
    }
    let (gets, kitchen) = match server.kitchen_of(&repo) {
        Ok(found) => found,
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
        "kitchen" => {
            let mut answer = json!({
                "kitchen": kitchen.name,
                "about": kitchen.about,
                "repository": repo,
                "parts": kitchen.parts,
                "behaviour": kitchen.behaviour,
                "concepts": listed(&kitchen.concepts),
                "recipes": listed(&kitchen.recipes),
                "tools": kitchen.tools.values().map(tool_listed).collect::<Vec<_>>(),
            });
            // What an agent in the kitchen's own repository goes by, to
            // offer to draft one that has nothing in it yet.
            if gets.why == Why::Itself {
                answer["own"] = true.into();
                // Drafting it is its maintainers': whoever else asks is not offered it.
                if std::env::var(ENV_ASKED_ROLE).as_deref() == Ok(MAINTAIN) {
                    answer["drafts"] = true.into();
                }
            }
            if kitchen::holds_nothing(&gets.dir) {
                answer["empty"] = true.into();
            }
            answer
        }
        // A tool, with what it takes to run it where the agent works: the
        // script, if the kitchen carries one.
        "tool" => {
            let name = params["name"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            match kitchen.tools.get(&name) {
                Some(tool) => {
                    let script = tool.spec.script.as_deref().map(|script| {
                        let text =
                            std::fs::read_to_string(tool.dir.join(script)).unwrap_or_default();
                        json!({ "name": script, "text": text })
                    });
                    json!({
                        "name": tool.name,
                        "about": tool.spec.about.trim(),
                        "command": tool.spec.command,
                        "script": script,
                        "args": tool.spec.args,
                        "input": input_of(tool),
                        "timeout_secs": tool.spec.timeout_secs,
                        "network": tool.spec.network,
                    })
                }
                None => {
                    let known: Vec<&str> = kitchen.tools.keys().map(String::as_str).collect();
                    let known = match known.is_empty() {
                        true => "it has none".to_owned(),
                        false => format!("it has {}", known.join(", ")),
                    };
                    json!({ "error": format!("the kitchen has no tool `{name}`: {known}") })
                }
            }
        }
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
    println!(
        "Tools: {}",
        if kitchen.tools.is_empty() { "none" } else { "" }
    );
    for tool in kitchen.tools.values() {
        let from = tool
            .part
            .as_deref()
            .map(|part| format!("  [{part}]"))
            .unwrap_or_default();
        let runs = tool.spec.script.as_deref().unwrap_or(&tool.spec.command);
        println!(
            "  {:<20} {}  (runs {runs}){from}",
            tool.name,
            tool.spec.about.trim()
        );
    }
}

/// Says which kitchen each repository gets, and why.
fn show_who_gets_what(all: &[(String, Option<Gets>)]) {
    println!("Which kitchen each repository gets:");
    for (repo, gets) in all {
        let said = gets.as_ref().map(|gets| {
            let called = kitchen::load(&gets.dir, &[]).map(|kitchen| kitchen.name);
            let called = called.unwrap_or_else(|_| "not a kitchen".to_owned());
            match gets.why {
                Why::Itself => format!("itself ({called}): it is a kitchen"),
                Why::Named => format!("`{}` ({called}): its own setting", gets.kitchen),
                Why::Servers => format!("`{}` ({called}): the server's", gets.kitchen),
            }
        });
        println!("  {repo:<20} {}", said.as_deref().unwrap_or("none"));
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
            println!("then `kito-server kitchen set kitchen` (for every repository), or `kito-server repo set <repository> --kitchen kitchen` (for one).");
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
        // The parts of the kitchen the repository gets.
        ["parts", repo, picked @ ..] => {
            let mut server = Server::open()?;
            server.repository(repo)?;
            let there = kitchen::parts_of(&server.gets(repo)?.dir);
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
            let all = server.who_gets_what();
            let server_s = server.config.repository.as_deref();
            match server.kitchen_dir() {
                Ok(dir) => {
                    let kitchen = kitchen::load(&dir, &[])?;
                    println!("Kitchen: {} (the repository `{}`)", kitchen.name, server_s.unwrap_or_default());
                    let parts = kitchen::parts_of(&dir);
                    println!("Parts: {}", if parts.is_empty() { "none".to_owned() } else { parts.join(", ") });
                    for (repo, picked) in &server.config.parts {
                        println!("  {repo} picked {}", picked.join(", "));
                    }
                    show(&kitchen);
                }
                // No kitchen anywhere: said as it was, with how to say which it is.
                Err(why) if all.iter().all(|(_, gets)| gets.is_none()) => return Err(why),
                Err(_) if server_s.is_none() => println!("No kitchen for the whole server (`kito-server kitchen set <repository>`)."),
                Err(why) => println!("No kitchen for the whole server: {why}."),
            }
            show_who_gets_what(&all);
            Ok(())
        }
        ["show", repo] => {
            let server = Server::open()?;
            server.repository(repo)?;
            let (gets, kitchen) = server.kitchen_of(repo)?;
            let parts = if kitchen.parts.is_empty() { "the base".to_owned() } else { format!("the base and {}", kitchen.parts.join(", ")) };
            println!("What `{repo}` gets of the kitchen {}: {parts}.", kitchen.name);
            println!("{}", gets.said(repo));
            println!("\n{}\n", if kitchen.behaviour.is_empty() { "(no behaviour written)" } else { &kitchen.behaviour });
            show(&kitchen);
            Ok(())
        }
        ["check"] => {
            let server = Server::open()?;
            let all = server.who_gets_what();
            // The kitchens there are: the server's, and those a repository gets.
            let mut kitchens: BTreeMap<String, PathBuf> = BTreeMap::new();
            let mut problems = Vec::new();
            match (server.kitchen_dir(), server.config.repository.clone()) {
                (Ok(dir), Some(name)) => drop(kitchens.insert(name, dir)),
                (Err(why), Some(_)) => problems.push(format!("the kitchen of the whole server: {why}")),
                _ => {}
            }
            for (_, gets) in &all {
                kitchens.extend(gets.iter().map(|gets| (gets.kitchen.clone(), gets.dir.clone())));
            }
            if kitchens.is_empty() {
                return Err(server.kitchen_dir().err().unwrap_or_default());
            }
            show_who_gets_what(&all);
            // Everything there is in each, and the repositories it says it is for.
            for (name, dir) in &kitchens {
                match kitchen::load(dir, &kitchen::parts_of(dir)) {
                    Ok(kitchen) => {
                        problems.extend(kitchen.problems.iter().map(|problem| format!("{name}/{problem}")));
                        for repo in kitchen.made_for.iter().filter(|repo| server.repository(repo).is_err()) {
                            problems.push(format!("{name}/kitchen.toml: it is for `{repo}`, which is not a repository of this server"));
                        }
                    }
                    Err(why) => {
                        let gotten_by = all.iter().filter(|(_, gets)| gets.as_ref().is_some_and(|gets| gets.kitchen == *name));
                        let gotten_by: Vec<&str> = gotten_by.map(|(repo, _)| repo.as_str()).collect();
                        problems.push(format!("{}: {why}", gotten_by.join(", ")));
                    }
                }
            }
            // What each repository names, and what it picked.
            for (repo, gets) in &all {
                let named = server.repository(repo).ok().and_then(|dir| Some((Server::named_in(&dir)?, dir)));
                let lost = named.filter(|(path, dir)| server.repository_at(dir, path).is_none());
                if let Some((path, _)) = lost.filter(|_| gets.as_ref().is_none_or(|gets| gets.why != Why::Itself)) {
                    problems.push(format!("{repo}: it names `{path}` as its kitchen, which is not a repository of this server (`kito-server repo set {repo} --kitchen <repository>`, or `--no-kitchen`)"));
                }
            }
            for (repo, picked) in &server.config.parts {
                let Ok(gets) = server.gets(repo) else { continue };
                let wrong = kitchen::load(&gets.dir, picked).map(|kitchen| kitchen.problems).unwrap_or_default();
                problems.extend(wrong.into_iter().filter(|problem| problem.starts_with("the part")).map(|problem| format!("{repo}: {problem}")));
            }
            problems.dedup();
            let several = kitchens.len() > 1;
            match problems.is_empty() {
                true => {
                    println!("{}", if several { "The kitchens are in order." } else { "The kitchen is in order." });
                    Ok(())
                }
                false => Err(format!("{} {}:\n  {}", if several { "the kitchens have" } else { "the kitchen has" }, count(problems.len(), "problem"), problems.join("\n  "))),
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
