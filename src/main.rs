mod cli;

use std::{env, fs};

use anyhow::bail;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use owo_colors::{OwoColorize as _, Stream};
use wrun::{Task, TaskName};

use self::cli::Action;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = cli::parse();

    let directory = if let Some(dir) = &args.directory {
        if !dir.is_dir() {
            bail!("{} is not a directory", dir.to_string_lossy());
        }

        fs::canonicalize(dir)?
    } else {
        env::current_dir()?
    };

    let context = wrun::Context::from_directory(directory)?;

    match args.action() {
        Action::List { all } => list_tasks(&context, all),
        Action::Run(tasks) => execute_tasks(context, tasks)?,
        Action::FetchTools => fetch_tools(&context).await?,
    }

    Ok(())
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

async fn fetch_tools(context: &wrun::Context) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct Progress(MultiProgress);
    struct Bar(ProgressBar);

    impl wrun::tools::DownloadProgress for Progress {
        type Bar = Bar;

        fn start(&self, name: &str, len: Option<u64>) -> Self::Bar {
            let bar = if let Some(len) = len {
                let style = ProgressStyle::with_template(
                    "{prefix:20}  {wide_bar} {binary_bytes_per_sec:>15} {eta:>5}",
                )
                .unwrap();
                ProgressBar::new(len).with_style(style)
            } else {
                let style = ProgressStyle::with_template("{prefix:20}  {spinner}").unwrap();
                ProgressBar::new_spinner().with_style(style)
            };
            let bar = bar.with_prefix(name.to_owned());
            Bar(self.0.add(bar))
        }
    }

    impl wrun::tools::DownloadProgressBar for Bar {
        fn update(&mut self, delta: u64) {
            self.0.inc(delta);
        }

        fn done(self) {
            self.0.finish();
        }
    }

    let progress = MultiProgress::new();
    let manager = context.fetch_tools();

    manager.run(Progress(progress)).await?;

    Ok(())
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
