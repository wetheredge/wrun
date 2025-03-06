use std::path::PathBuf;
use std::{env, fs};

use anyhow::bail;
use clap::Parser as _;

#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(long, aliases = ["cwd", "dir"])]
    directory: Option<PathBuf>,
    tasks: Vec<String>,
}

fn main() -> anyhow::Result<()> {
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
    let abs_task = |task| wrun::TaskName::new(task).relative_to(&local_package);
    let mut plan = context.plan();
    for task in &args.tasks {
        plan.push(&abs_task(task))?;
    }
    plan.execute()?;

    Ok(())
}
