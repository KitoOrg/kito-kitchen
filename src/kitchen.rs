//! A kitchen as it is kept: a directory of files.
//!
//! ```text
//! kitchen.toml            what the kitchen is called
//! behaviour.md            how agents work here: always told
//! concepts/<name>.md      what a word of the team means: looked up
//! recipes/<name>.md       how something is done, step by step: looked up
//! tools/<name>.toml       a command every agent has, and what it takes
//! parts/<part>/…          the same, for the repositories that pick the part
//! ```
//!
//! A repository gets the base and the parts it picked, a part winning
//! over the base where both have something of the same name.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The longest line that says what a concept or a recipe is about.
const ABOUT_AT_MOST: usize = 160;

#[derive(Default, Deserialize)]
#[serde(default)]
struct Manifest {
    name: String,
    about: String,
}

/// A concept or a recipe: its name, the line that says what it is about,
/// and where its text is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub name: String,
    pub about: String,
    /// The part it comes from; none: the base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    #[serde(skip)]
    pub file: PathBuf,
}

/// A tool as the kitchen writes it: `tools/<name>.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ToolSpec {
    /// What it does and when to use it, for the agent.
    pub about: String,
    /// The program to run, found as the person's machine finds programs
    /// or in the lane's working copy; or else `script`.
    pub command: String,
    /// A file next to this one, which the kitchen carries: run instead of
    /// a program the machine has.
    pub script: Option<String>,
    /// What it is given: each a word of its own, never a line for a
    /// shell. `{name}` is what the agent gave under that name.
    pub args: Vec<String>,
    /// The JSON Schema of what the agent gives (an object).
    pub input: Option<toml::Value>,
    /// Seconds after which it is stopped (default 120).
    pub timeout_secs: Option<u64>,
}

/// A tool of the kitchen: a command every agent of the team has.
#[derive(Clone, Debug)]
pub struct Tool {
    pub name: String,
    pub part: Option<String>,
    /// Where its file is: its script is next to it.
    pub dir: PathBuf,
    pub spec: ToolSpec,
}

/// What one repository gets of a kitchen.
#[derive(Debug, Default)]
pub struct Kitchen {
    pub name: String,
    pub about: String,
    /// The parts it was put together from, after the base.
    pub parts: Vec<String>,
    /// How agents work here: the base's, then each part's under its name.
    pub behaviour: String,
    pub concepts: BTreeMap<String, Entry>,
    pub recipes: BTreeMap<String, Entry>,
    pub tools: BTreeMap<String, Tool>,
    /// What is wrong with it, said to whoever checks it: none of it stops
    /// the rest from being given.
    pub problems: Vec<String>,
}

/// `true` for a name a concept, a recipe or a part can have: what goes in
/// a file's name and in a command, and nowhere else.
pub fn is_name(name: &str) -> bool {
    let fits = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
    !name.is_empty() && name.len() <= 64 && name.chars().all(fits) && !name.starts_with('-')
}

/// What a text is about: its first line that says something, without the
/// marks of a heading.
pub fn about_of(text: &str) -> String {
    let line = text
        .lines()
        .map(|line| line.trim_start_matches('#').trim())
        .find(|line| !line.is_empty());
    let line = line.unwrap_or_default();
    match line.char_indices().nth(ABOUT_AT_MOST) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

/// The parts a kitchen has: the directories of `parts/`.
pub fn parts_of(dir: &Path) -> Vec<String> {
    let mut parts: Vec<String> = std::fs::read_dir(dir.join("parts"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| is_name(name))
        .collect();
    parts.sort();
    parts
}

/// The concepts or the recipes of one directory (`concepts`, `recipes`),
/// added to `all`: what is there already under the same name is replaced.
fn entries(
    from: &Path,
    kind: &str,
    part: Option<&str>,
    all: &mut BTreeMap<String, Entry>,
    problems: &mut Vec<String>,
) {
    let dir = from.join(kind);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    files.sort();
    for file in files {
        let shown = file
            .strip_prefix(from)
            .unwrap_or(&file)
            .display()
            .to_string();
        let shown = part.map_or(shown.clone(), |part| format!("parts/{part}/{shown}"));
        if file.extension().and_then(|ext| ext.to_str()) != Some("md") || !file.is_file() {
            continue;
        }
        let name = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !is_name(&name) {
            problems.push(format!(
                "{shown}: `{name}` is not a name (small letters, digits, `-` and `_`)"
            ));
            continue;
        }
        let about = std::fs::read_to_string(&file)
            .map(|text| about_of(&text))
            .unwrap_or_default();
        if about.is_empty() {
            problems.push(format!(
                "{shown}: it says nothing: its first line is what an agent is told it is about"
            ));
            continue;
        }
        all.insert(
            name.clone(),
            Entry {
                name,
                about,
                part: part.map(str::to_owned),
                file,
            },
        );
    }
}

/// The tools of one directory (`tools/<name>.toml`), added to `all`.
fn tools(
    from: &Path,
    part: Option<&str>,
    all: &mut BTreeMap<String, Tool>,
    problems: &mut Vec<String>,
) {
    let dir = from.join("tools");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    files.sort();
    for file in files
        .iter()
        .filter(|file| file.extension().and_then(|ext| ext.to_str()) == Some("toml"))
    {
        let name = file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let shown = match part {
            Some(part) => format!("parts/{part}/tools/{name}.toml"),
            None => format!("tools/{name}.toml"),
        };
        if !is_name(&name) {
            problems.push(format!(
                "{shown}: `{name}` is not a name (small letters, digits, `-` and `_`)"
            ));
            continue;
        }
        let read = std::fs::read_to_string(file).map_err(|e| e.to_string());
        let spec: ToolSpec =
            match read.and_then(|text| toml::from_str(&text).map_err(|e| e.to_string())) {
                Ok(spec) => spec,
                Err(why) => {
                    problems.push(format!(
                        "{shown}: {}",
                        why.lines().next().unwrap_or_default()
                    ));
                    continue;
                }
            };
        // A script is a file of the kitchen next to the tool, and nothing else.
        let script_fits = spec.script.as_deref().is_none_or(|script| {
            !script.contains(['/', '\\']) && !script.starts_with('.') && dir.join(script).is_file()
        });
        let wrong = if spec.about.trim().is_empty() {
            Some("it has no `about`: that is what tells an agent when to use it")
        } else if spec.command.trim().is_empty() == spec.script.is_none() {
            Some("it names a `command` or a `script`, one of the two")
        } else if !script_fits {
            Some("its `script` is not a file next to it")
        } else {
            None
        };
        match wrong {
            Some(wrong) => problems.push(format!("{shown}: {wrong}")),
            None => {
                let tool = Tool {
                    name: name.clone(),
                    part: part.map(str::to_owned),
                    dir: dir.clone(),
                    spec,
                };
                all.insert(name, tool);
            }
        }
    }
}

/// Reads what a repository that picked `picked` gets of the kitchen in
/// `dir`. `Err`: the directory is no kitchen.
pub fn load(dir: &Path, picked: &[String]) -> Result<Kitchen, String> {
    let manifest = dir.join("kitchen.toml");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|_| format!("{} is not a kitchen: it has no kitchen.toml", dir.display()))?;
    let manifest: Manifest =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let mut kitchen = Kitchen {
        name: manifest.name.trim().to_owned(),
        about: manifest.about.trim().to_owned(),
        ..Kitchen::default()
    };
    if kitchen.name.is_empty() {
        kitchen
            .problems
            .push("kitchen.toml: it has no `name`".to_owned());
        kitchen.name = "the team".to_owned();
    }
    let behaviour = |from: &Path| {
        std::fs::read_to_string(from.join("behaviour.md"))
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    kitchen.behaviour = behaviour(dir);
    entries(
        dir,
        "concepts",
        None,
        &mut kitchen.concepts,
        &mut kitchen.problems,
    );
    entries(
        dir,
        "recipes",
        None,
        &mut kitchen.recipes,
        &mut kitchen.problems,
    );
    tools(dir, None, &mut kitchen.tools, &mut kitchen.problems);
    for part in picked {
        let from = dir.join("parts").join(part);
        if !is_name(part) || !from.is_dir() {
            kitchen.problems.push(format!(
                "the part `{part}` is picked, and the kitchen has no parts/{part}/"
            ));
            continue;
        }
        let more = behaviour(&from);
        if !more.is_empty() {
            let gap = if kitchen.behaviour.is_empty() {
                ""
            } else {
                "\n\n"
            };
            kitchen
                .behaviour
                .push_str(&format!("{gap}## {part}\n\n{more}"));
        }
        entries(
            &from,
            "concepts",
            Some(part),
            &mut kitchen.concepts,
            &mut kitchen.problems,
        );
        entries(
            &from,
            "recipes",
            Some(part),
            &mut kitchen.recipes,
            &mut kitchen.problems,
        );
        tools(&from, Some(part), &mut kitchen.tools, &mut kitchen.problems);
        kitchen.parts.push(part.clone());
    }
    Ok(kitchen)
}

/// A kitchen to start from, written into an empty directory.
pub fn start(dir: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let said = |e: std::io::Error| format!("{}: {e}", dir.display());
    std::fs::create_dir_all(dir).map_err(said)?;
    if std::fs::read_dir(dir)
        .map_err(said)?
        .flatten()
        .any(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
    {
        return Err(format!(
            "{} is not empty: a kitchen is started in a directory of its own",
            dir.display()
        ));
    }
    let files = [
        ("kitchen.toml", format!("name = {}\nabout = \"How we work on our repositories.\"\n", toml::Value::String(name.to_owned()))),
        (
            "behaviour.md",
            "Say what you are about to do before you do it, in one sentence.\n\nRun the tests of what you changed \
             before you ask for an integration, and say what you ran.\n\nWhen something here is unclear, look in \
             the kitchen (`concept`, `recipe`) before you guess.\n"
                .to_owned(),
        ),
        (
            "concepts/kitchen.md",
            "# What this team shares to work on its repositories\n\nThe kitchen is where we write down how we \
             work: `behaviour.md` is told to every agent, concepts say what our words mean, recipes say how \
             things are done here. It is a repository: change it through a lane, like code.\n"
                .to_owned(),
        ),
        (
            "recipes/add-a-recipe.md",
            "# How a recipe is added to the kitchen\n\n1. Open a lane in the kitchen's repository.\n2. Write \
             `recipes/<name>.md`: the first line says what it is for, the rest is the steps.\n3. Ask for the \
             integration. Once it is in, every agent of the team can ask for it.\n"
                .to_owned(),
        ),
        ("parts/README.md", "A directory here is a part (`parts/rust/`): the same files as the kitchen has, for the repositories that pick it (`kito-server kitchen parts <repository> rust`).\n".to_owned()),
    ];
    let mut written = Vec::new();
    for (name, text) in files {
        let file = dir.join(name);
        std::fs::create_dir_all(file.parent().unwrap_or(dir)).map_err(said)?;
        std::fs::write(&file, text).map_err(said)?;
        written.push(file);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, file: &str, text: &str) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_repository_gets_the_base_and_the_parts_it_picked() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        write(dir, "kitchen.toml", "name = \"Shopworks\"\n");
        write(dir, "behaviour.md", "Say what you ran.\n");
        write(
            dir,
            "concepts/money.md",
            "# Amounts are whole cents\n\nNever a float.\n",
        );
        write(
            dir,
            "concepts/order.md",
            "An order is what a customer paid for.\n",
        );
        write(
            dir,
            "recipes/release.md",
            "# How a release is made\n\n1. Tag.\n",
        );
        write(dir, "parts/rust/behaviour.md", "Run `cargo clippy`.\n");
        write(
            dir,
            "parts/rust/concepts/money.md",
            "# Amounts are `Cents(i64)`\n",
        );
        write(
            dir,
            "parts/rust/recipes/add-a-crate.md",
            "# How a crate is added\n",
        );
        write(dir, "parts/web/concepts/page.md", "# A page is a route\n");

        let base = load(dir, &[]).unwrap();
        assert_eq!(
            (base.name.as_str(), base.behaviour.as_str()),
            ("Shopworks", "Say what you ran.")
        );
        assert_eq!(base.concepts.keys().collect::<Vec<_>>(), ["money", "order"]);
        assert_eq!(base.concepts["money"].about, "Amounts are whole cents");
        assert_eq!(
            base.concepts["order"].about,
            "An order is what a customer paid for."
        );
        assert!(base.problems.is_empty(), "{:?}", base.problems);
        assert_eq!(parts_of(dir), ["rust", "web"]);

        // A part adds, and wins where the base has the same name.
        let rust = load(dir, &["rust".to_owned()]).unwrap();
        assert_eq!(
            rust.behaviour,
            "Say what you ran.\n\n## rust\n\nRun `cargo clippy`."
        );
        assert_eq!(rust.concepts["money"].about, "Amounts are `Cents(i64)`");
        assert_eq!(rust.concepts["money"].part.as_deref(), Some("rust"));
        assert_eq!(
            rust.recipes.keys().collect::<Vec<_>>(),
            ["add-a-crate", "release"]
        );
        assert!(!rust.concepts.contains_key("page"));

        // Tools: a command the machine has, or a script the kitchen carries.
        let test = "about = \"Runs the tests\"\ncommand = \"npm\"\nargs = [\"test\", \"--\", \"{filter}\"]\n";
        write(dir, "tools/test.toml", test);
        write(dir, "tools/lint.sh", "#!/bin/sh\necho ok\n");
        write(
            dir,
            "tools/lint.toml",
            "about = \"Checks the style\"\nscript = \"lint.sh\"\n",
        );
        write(
            dir,
            "tools/both.toml",
            "about = \"x\"\ncommand = \"a\"\nscript = \"lint.sh\"\n",
        );
        write(
            dir,
            "tools/out.toml",
            "about = \"x\"\nscript = \"../behaviour.md\"\n",
        );
        write(dir, "tools/mute.toml", "command = \"a\"\n");
        let cargo =
            "about = \"Runs the tests of the crate\"\ncommand = \"cargo\"\nargs = [\"test\"]\n";
        write(dir, "parts/rust/tools/test.toml", cargo);
        let with = load(dir, &["rust".to_owned()]).unwrap();
        assert_eq!(with.tools.keys().collect::<Vec<_>>(), ["lint", "test"]);
        let test = &with.tools["test"];
        assert_eq!(
            (test.spec.command.as_str(), test.part.as_deref()),
            ("cargo", Some("rust"))
        );
        assert_eq!(with.tools["lint"].spec.script.as_deref(), Some("lint.sh"));
        assert_eq!(
            load(dir, &[]).unwrap().tools["test"].spec.args,
            ["test", "--", "{filter}"]
        );
        assert_eq!(with.problems.len(), 3, "{:?}", with.problems);
        for file in ["both", "out", "mute"] {
            std::fs::remove_file(dir.join(format!("tools/{file}.toml"))).unwrap();
        }

        // What is wrong is said, and the rest is given.
        write(dir, "concepts/Bad Name.md", "x\n");
        write(dir, "concepts/empty.md", "\n\n");
        let odd = load(dir, &["rust".to_owned(), "go".to_owned()]).unwrap();
        assert_eq!(odd.problems.len(), 3, "{:?}", odd.problems);
        assert!(odd
            .problems
            .iter()
            .any(|problem| problem.contains("the kitchen has no parts/go/")));
        assert_eq!(odd.concepts.len(), 2);
        assert!(load(&dir.join("concepts"), &[])
            .unwrap_err()
            .contains("is not a kitchen"));
    }

    #[test]
    fn names_are_what_goes_in_a_file_s_name_and_nowhere_else() {
        assert!(is_name("money") && is_name("add-a-crate") && is_name("v2_api"));
        for bad in ["", "../etc", "a/b", "Money", "-x", "a b", "a.md"] {
            assert!(!is_name(bad), "{bad}");
        }
        assert_eq!(
            about_of("\n## How a release is made  \n\nsteps"),
            "How a release is made"
        );
        assert_eq!(about_of(&"é".repeat(200)).chars().count(), 161);
    }

    #[test]
    fn a_kitchen_to_start_from_is_a_kitchen() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("kitchen");
        assert_eq!(start(&dir, "Shop \"works\"").unwrap().len(), 5);
        let kitchen = load(&dir, &[]).unwrap();
        assert_eq!(kitchen.name, "Shop \"works\"");
        assert!(
            kitchen.problems.is_empty()
                && kitchen.concepts.len() == 1
                && kitchen.recipes.len() == 1
        );
        assert!(start(&dir, "again").unwrap_err().contains("is not empty"));
    }
}
