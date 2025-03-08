use std::ffi::OsStr;
use std::path::PathBuf;
use std::{env, fs};

use anyhow::bail;
use clap::{CommandFactory as _, Parser as _, ValueHint};
use clap_complete::CompletionCandidate;
use clap_complete::engine::ArgValueCompleter;
use owo_colors::{OwoColorize as _, Stream};
use wrun::{Task, TaskName};

#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(long, aliases = ["cwd", "dir"])]
    #[clap(value_hint = ValueHint::DirPath)]
    directory: Option<PathBuf>,

    #[clap(short, long)]
    all: bool,

    #[clap(add = ArgValueCompleter::new(TaskCompleter))]
    tasks: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let is_completion = clap_complete::CompleteEnv::with_factory(Args::command)
        .try_complete(env::args_os(), env::current_dir().ok().as_deref())
        .unwrap();
    if is_completion {
        return Ok(());
    }

    let args = Args::parse();

    let directory = if let Some(dir) = args.directory {
        if !dir.is_dir() {
            bail!("{} is not a directory", dir.to_string_lossy());
        }

        fs::canonicalize(dir)?
    } else {
        env::current_dir()?
    };

    let context = wrun::Context::from_directory(directory)?;

    if args.tasks.is_empty() {
        list_tasks(&context, args.all);

        return Ok(());
    }

    execute_tasks(context, &args.tasks)
}

fn list_tasks(context: &wrun::Context, all: bool) {
    let is_public = |t: &(_, &Task)| !t.1.is_internal();
    let print_task = |name: &str, task: &Task| {
        let name = name.if_supports_color(Stream::Stdout, |s| s.purple());
        println!("  {name:18}  {}", task.description().unwrap_or_default())
    };

    println!("Local:");

    for (name, task) in context.local_tasks().iter().filter(is_public) {
        print_task(name, task);
    }

    if all {
        let local = context.local_package_name();
        for (name, package) in context.packages() {
            if name == local {
                continue;
            }

            let mut tasks = package.tasks().iter().filter(is_public).peekable();
            if tasks.peek().is_some() {
                let name = &format!("{name}/");
                let name = name.if_supports_color(Stream::Stdout, |s| s.blue());
                println!("In {name}:");
                for (name, task) in tasks {
                    print_task(name, task);
                }
            }
        }
    }
}

fn execute_tasks(mut context: wrun::Context, tasks: &[String]) -> anyhow::Result<()> {
    let local_package = context.local_package_name().to_owned();
    let abs_task = |task| TaskName::new(task).relative_to(&local_package);
    let mut plan = context.plan();
    for task in tasks {
        plan.push(&abs_task(task))?;
    }
    plan.execute(|entry| {
        if !entry.silent() {
            let task = entry.task();
            let task = task.if_supports_color(Stream::Stderr, |s| s.purple());
            eprintln!("wrun({task}): {}", entry.command());
        }
    })?;

    Ok(())
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
