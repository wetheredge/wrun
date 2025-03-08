use std::ffi::OsStr;
use std::path::PathBuf;
use std::{env, fs};

use anyhow::bail;
use clap::{CommandFactory as _, Parser as _, ValueHint};
use clap_complete::CompletionCandidate;
use clap_complete::engine::ArgValueCompleter;
use wrun::TaskName;

#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(long, aliases = ["cwd", "dir"])]
    #[clap(value_hint = ValueHint::DirPath)]
    directory: Option<PathBuf>,
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

    let mut context = wrun::Context::from_directory(directory)?;

    if args.tasks.is_empty() {
        let tasks = context.local_tasks();

        println!("Local tasks:");
        for (name, task) in tasks.iter() {
            if !task.is_internal() {
                println!("  {name:18}  {}", task.description().unwrap_or_default());
            }
        }

        return Ok(());
    }

    let local_package = context.local_package_name().to_owned();
    let abs_task = |task| TaskName::new(task).relative_to(&local_package);
    let mut plan = context.plan();
    for task in &args.tasks {
        plan.push(&abs_task(task))?;
    }
    plan.execute()?;

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
            let mut context = wrun::Context::from_directory(dir).ok()?;

            let help = |task: &wrun::Task| task.description().map(|s| s.to_owned().into());

            let mut candidates = Vec::new();
            for (name, task) in context.local_tasks().iter() {
                candidates.push(CompletionCandidate::new(name).help(help(task)));
            }

            let local = context.local_package_name().to_owned();
            for (package_name, package) in context.all_packages() {
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
