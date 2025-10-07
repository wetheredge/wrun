mod data;
mod vec_map;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::rc::Rc;

use anyhow::{Context as _, bail};

use self::data::Package;
pub use self::data::{AbsoluteTaskName, Task, TaskName, Tasks};
use self::vec_map::VecMap;

const PROJECT_FILE: &str = "wrun-project.toml";
const PACKAGE_FILE: &str = "wrun.toml";

#[derive(Debug)]
pub struct Context {
    root: PathBuf,
    env_files: Vec<PathBuf>,
    local: Option<String>,
    packages: VecMap<Package>,
}

impl Context {
    pub fn from_directory(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut local_package_dir = None;
        let mut root = None;

        for dir in dir.as_ref().ancestors() {
            let try_package = dir.join(PACKAGE_FILE);
            if fs::exists(&try_package)? && local_package_dir.is_none() {
                local_package_dir = Some(dir);
            }

            let try_project = dir.join(PROJECT_FILE);
            if fs::exists(&try_project)? {
                root = Some(dir.to_path_buf());
                break;
            }
        }
        let Some(root) = root else {
            bail!("failed to find project root")
        };

        let data::Project {
            env_files,
            packages,
            package: root_package,
        } = toml_from_path(&root.join(PROJECT_FILE))?;
        let mut context = Self {
            root,
            env_files,
            local: None,
            packages: VecMap::default(),
        };
        context.packages.insert(String::new(), root_package);

        for dir in packages {
            let name = dir.to_string_lossy().into_owned();
            let package = context
                .load_package(&dir)
                .with_context(|| format!("loading package from {}", dir.display()))?;
            context.packages.insert(name, package);
        }

        if let Some(path) = local_package_dir {
            let relative = path.strip_prefix(&context.root).unwrap();
            let name = relative.to_string_lossy();

            if context.packages.get_index(&name).is_none() {
                bail!(
                    "Not in packages list defined by project in `{}`",
                    &context.root.display(),
                );
            }

            context.local = Some(name.into_owned());
        }

        Ok(context)
    }

    fn load_package(&self, path: &Path) -> anyhow::Result<Package> {
        toml_from_path(&self.root.join(path).join(PACKAGE_FILE))
    }

    fn get_package<'a>(&'a mut self, name: &str) -> anyhow::Result<&'a Package> {
        // TODO: use regular .get() once rust allows the borrow to end with the if
        if let Some(index) = self.packages.get_index(name) {
            return Ok(self.packages.get_by_index(index).unwrap());
        }

        let package = self.load_package(Path::new(name))?;
        let package = self.packages.insert(name.to_owned(), package);
        Ok(package)
    }

    pub fn local_package_name(&self) -> &str {
        if let Some(local) = &self.local {
            local
        } else {
            ""
        }
    }

    pub fn local_tasks(&self) -> &Tasks {
        let package = self.local.as_deref().unwrap_or("");
        &self.packages.get(package).unwrap().tasks
    }

    pub fn packages(&self) -> impl Iterator<Item = (&str, &Package)> {
        self.packages.iter()
    }

    pub fn plan(&mut self) -> Plan<'_> {
        Plan::new(self)
    }

    fn dotenv(&self) -> anyhow::Result<impl Iterator<Item = (String, String)>> {
        let mut env = HashMap::new();
        for path in &self.env_files {
            let path = self.root.join(path);
            if fs::exists(&path)? {
                for entry in dotenvy::from_path_iter(path)? {
                    let entry = entry?;
                    env.insert(entry.0, entry.1);
                }
            }
        }
        Ok(env.into_iter())
    }
}

#[derive(Debug)]
pub struct Plan<'a> {
    context: &'a mut Context,
    plan: Vec<PlanEntry>,
}

impl<'a> Plan<'a> {
    fn new(context: &'a mut Context) -> Self {
        Self {
            context,
            plan: Vec::new(),
        }
    }

    pub fn push(&mut self, task_name: &AbsoluteTaskName) -> anyhow::Result<()> {
        let package_name = task_name.package();
        let package = self.context.get_package(package_name)?;

        let Some(task) = package.tasks.0.get(task_name.task()) else {
            bail!("Cannot find task: {task_name}")
        };
        let task = Rc::clone(task);

        for run in &task.run {
            match run {
                data::Run::Command { command, silent } => {
                    self.plan.push(PlanEntry {
                        task: task_name.clone(),
                        directory: self.context.root.join(package_name),
                        command: command.clone(),
                        silent: silent.unwrap_or(task.is_silent()),
                    });
                }
                data::Run::Task(task) => self.push(&task.clone().relative_to(package_name))?,
            }
        }

        Ok(())
    }

    pub fn execute(self, prerun: impl Fn(&PlanEntry)) -> anyhow::Result<()> {
        let wrun_bin = std::env::current_exe().expect("path to wrun");

        for entry in &self.plan {
            prerun(entry);

            let exit = Command::new("sh")
                .current_dir(&*entry.directory)
                .envs(self.context.dotenv()?)
                .env("WRUN", &wrun_bin)
                .env("ROOT", &self.context.root)
                .args(["-c", entry.command()])
                .status()?;

            if !exit.success() {
                let code = exit.code().unwrap(); // FIXME
                process::exit(code)
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct PlanEntry {
    task: AbsoluteTaskName,
    directory: PathBuf,
    command: String,
    silent: bool,
}

impl PlanEntry {
    pub fn task(&self) -> &AbsoluteTaskName {
        &self.task
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn silent(&self) -> bool {
        self.silent
    }
}

fn toml_from_path<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}
