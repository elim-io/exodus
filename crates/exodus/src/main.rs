//! The `exodus` command line: audit a repository, price it on Cloud Run, build the report and move it with your agent.

mod commands;
mod platform;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "exodus",
    version,
    about = "Audit a repository and move it to Google Cloud Run with your coding agent"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lay out .exodus/ and the agent files; link an account with --code
    Init(commands::init::Args),
    /// Link this machine to this repository's project with a new code
    Login(commands::init::LoginArgs),
    /// Show the project, the key id, reports and plan progress
    Status(commands::status::Args),
    /// Read the manifests and the code into .exodus/findings.json
    Audit(commands::audit::Args),
    /// Answer one of the audit's questions
    Answer(commands::answer::Args),
    /// Write what you understood from the code (stdin or --file) at the top of the report
    Describe(commands::describe::Args),
    /// Print the JSON Schema of findings, answers or report
    #[command(hide = true)] // not built yet; hidden so the help lists only commands that work
    Schema,
    /// Upload findings and answers to the platform and pull the report back (the only command that uploads)
    Send(commands::send::Args),
    /// Build the report here from the findings and answers, or --pull the platform's latest
    Report {
        #[command(subcommand)]
        command: Option<ReportCommand>,
        #[command(flatten)]
        args: commands::report::Args,
    },
    /// Write .exodus/plan.md from a report
    Plan(commands::plan::Args),
    /// Work with the plan's tasks
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Ask the person for something a task cannot go on without
    Need(commands::plan::NeedArgs),
    /// Mint a second key for this project, for CI
    Key(commands::key::Args),
    /// Read last month's Google Cloud bill for the project
    Bill(commands::bill::Args),
    /// Look for what breaks or idles on Cloud Run, for CI
    Check(commands::check::Args),
    /// Read an article of the knowledge base, or list a topic's
    Kb(commands::kb::KbArgs),
    /// Search the knowledge base: the best passages for a question, with their links
    Ask(commands::kb::AskArgs),
}

#[derive(Subcommand)]
enum ReportCommand {
    /// Pull a report version from the platform (the same as --pull)
    #[command(hide = true)]
    Pull(commands::send::PullArgs),
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Tick a task in the plan, with the commit that did it
    Done(commands::plan::DoneArgs),
}

fn main() -> std::process::ExitCode {
    match Cli::parse().command {
        Command::Init(args) => commands::init::run(args),
        Command::Login(args) => commands::init::login(args),
        Command::Status(args) => commands::status::run(args),
        Command::Audit(args) => commands::audit::run(args),
        Command::Answer(args) => commands::answer::run(args),
        Command::Describe(args) => commands::describe::run(args),
        Command::Schema => {
            eprintln!("exodus schema: not yet");
            std::process::ExitCode::FAILURE
        }
        Command::Send(args) => commands::send::run(args),
        Command::Report { command: None, args } => commands::report::run(args),
        Command::Report {
            command: Some(ReportCommand::Pull(args)),
            ..
        } => commands::send::pull_command(args),
        Command::Plan(args) => commands::plan::plan(args),
        Command::Task {
            command: TaskCommand::Done(args),
        } => commands::plan::done(args),
        Command::Need(args) => commands::plan::need(args),
        Command::Key(args) => commands::key::run(args),
        Command::Bill(args) => commands::bill::run(args),
        Command::Check(args) => commands::check::run(args),
        Command::Kb(args) => commands::kb::kb(args),
        Command::Ask(args) => commands::kb::ask(args),
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }

    /// The agent instructions name only commands this binary has.
    #[test]
    fn templates_name_real_commands() {
        let cli = super::Cli::command();
        for text in [crate::commands::init::MOVE, crate::commands::init::NEW] {
            for name in text
                .split("`exodus ")
                .skip(1)
                .filter_map(|s| s.split([' ', '`']).next())
            {
                assert!(
                    cli.find_subcommand(name).is_some(),
                    "the templates name `exodus {name}`"
                );
            }
        }
    }
}
