use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;

use clap::{CommandFactory, Parser, ValueHint};
use clap_complete::CompletionCandidate;
use clap_complete::engine::ArgValueCompleter;

pub(crate) fn parse() -> Args {
    clap_complete::CompleteEnv::with_factory(Args::command).complete();
    Args::parse()
}

static AFTER_SHORT_HELP: &str = "By default, wrun prints the list of local tasks";
static AFTER_LONG_HELP: &str = "By default, wrun prints the list of local tasks.

Tasks can be specified in 3 ways:
- foo: The task `foo` defined in the current package (a directory with a wrun.toml file)
- bar/baz: `baz` from the package `bar`
- /quux: `quux` from the project root";

#[derive(Debug, Parser)]
#[command(about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(after_help = AFTER_SHORT_HELP)]
#[command(after_long_help = AFTER_LONG_HELP)]
pub struct Args {
    #[clap(long, aliases = ["cwd", "dir"])]
    #[clap(value_hint = ValueHint::DirPath)]
    pub(crate) directory: Option<PathBuf>,

    #[command(flatten)]
    action: ActionArgs,
}

#[derive(Debug, clap::Args)]
#[group(multiple = false)]
struct ActionArgs {
    /// List all tasks, not just local ones
    #[clap(short, long)]
    all: bool,

    /// Run one or more tasks
    #[clap(add = ArgValueCompleter::new(TaskCompleter))]
    tasks: Vec<String>,
}

#[derive(Debug)]
pub(crate) enum Action<'a> {
    List { all: bool },
    Run(&'a [String]),
}

impl Args {
    pub(crate) fn action(&self) -> Action {
        let action = &self.action;

        if action.all {
            Action::List { all: true }
        } else if action.tasks.is_empty() {
            Action::List { all: false }
        } else {
            Action::Run(&action.tasks)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct TaskCompleter;

impl clap_complete::engine::ValueCompleter for TaskCompleter {
    fn complete(&self, current: &OsStr) -> Vec<CompletionCandidate> {
        let inner = || -> Option<_> {
            let current = current.to_str()?;

            let valid_task_char = |c: char| c.is_alphanumeric() || c == '/' || c == ':' || c == '_';
            if !(current.is_empty() || current.starts_with(valid_task_char)) {
                return None;
            }

            // TODO: Take --directory into account
            let dir = env::current_dir().ok()?;
            let context = wrun::Context::from_directory(dir).ok()?;

            let help = |task: &wrun::Task| task.description().map(|s| s.to_owned().into());

            let mut candidates = Vec::new();
            for (name, task) in context.local_tasks().iter() {
                candidates.push(CompletionCandidate::new(name).help(help(task)));
            }

            let local = context.local_package_name();
            for (package_name, package) in context.packages() {
                for (name, task) in package.tasks().iter() {
                    candidates.push(
                        CompletionCandidate::new(format!("{package_name}/{name}"))
                            .help(help(task))
                            .hide(package_name == local),
                    );
                }
            }

            Some(candidates)
        };

        inner().unwrap_or_default()
    }
}
