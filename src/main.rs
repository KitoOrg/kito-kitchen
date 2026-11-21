//! `kito-kitchen`: a team's kitchen (ADR 0007 of Kito).
//!
//! A kitchen is what a team shares to work on its repositories: how its
//! agents behave, what its words mean (concepts), how things are done
//! (recipes). It is a repository on the team's Kito server, and every
//! agent of every person gets it through Kito, whatever assistant they
//! use. One program, on both sides:
//!
//! - on the server, `kito-server kitchen …` says which repository is the
//!   kitchen and which parts each repository picks, and `kito-kitchen
//!   answer` is what the server runs for a question from a client;
//! - in a directory connected to the server, `kito kitchen …` shows it to
//!   a person, and `kito-kitchen tool …` gives it to an agent, who is
//!   told the behaviour when a session starts (`kito-kitchen session`).

mod client;
mod kitchen;
mod server;

use serde_json::Value;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let done = match words.as_slice() {
        // Run by the server for a question of the same plugin on a
        // client's machine: the answer is the last line printed.
        ["answer", question] => {
            let params: Value = serde_json::from_reader(std::io::stdin()).unwrap_or(Value::Null);
            server::answer(question, &params).map(|answer| println!("{answer}"))
        }
        // As a command of the server, it is given its own name first.
        ["kitchen", rest @ ..]
            if std::env::var_os("KITO_SERVER_DIR").is_some()
                && std::env::var_os("KITO_SOCKET").is_none() =>
        {
            server::admin(
                &rest
                    .iter()
                    .map(|word| (*word).to_owned())
                    .collect::<Vec<_>>(),
            )
        }
        // What an agent is told when a session starts.
        ["session"] => {
            print!("{}", client::session());
            Ok(())
        }
        // A tool of the plugin, for an agent.
        ["tool", name] => {
            println!("{}", client::tool(name));
            Ok(())
        }
        ["--version"] => {
            println!("kito-kitchen {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        // `kito kitchen …`, for a person: a command of a repository is not
        // given its name.
        rest => client::show(rest),
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("kito-kitchen: {why}");
            ExitCode::from(1)
        }
    }
}
