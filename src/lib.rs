mod data;
mod vec_map;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::rc::Rc;

use anyhow::bail;
use regex::Regex;

pub use self::data::{AbsoluteTaskName, Task, TaskName, Tasks};
use self::data::{Package, Tool};
use self::vec_map::VecMap;

const PROJECT_FILE: &str = "wrun-project.toml";
const PACKAGE_FILE: &str = "wrun.toml";

#[derive(Debug)]
pub struct Context {
    root: PathBuf,
    env_files: Vec<PathBuf>,
    tools: VecMap<Tool>,
    local: Option<String>,
    packages: VecMap<Package>,
}

impl Context {
    pub fn from_directory(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut package_dir = None;
        let mut root = None;

        for dir in dir.as_ref().ancestors() {
            let try_package = dir.join(PACKAGE_FILE);
            if fs::exists(&try_package)? && package_dir.is_none() {
                package_dir = Some(dir);
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
            tools,
            package: root_package,
        } = toml_from_path(&root.join(PROJECT_FILE))?;
        let mut context = Self {
            root,
            env_files,
            tools,
            local: None,
            packages: VecMap::default(),
        };
        context.packages.insert(String::new(), root_package);

        if let Some(path) = package_dir {
            let relative = path.strip_prefix(&context.root).unwrap();
            let name = relative.to_string_lossy().into_owned();
            let package = context.load_package(relative)?;
            context.packages.insert(name.clone(), package);
            context.local = Some(name);
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

    pub fn all_packages(&mut self) -> impl Iterator<Item = (&str, &Package)> {
        self.packages.iter()
    }

    pub fn plan(&mut self) -> Plan {
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

    fn tool_env(&self) -> VecMap<String> {
        self.tools
            .iter()
            .map(|(name, tool)| {
                let name = name.to_owned();
                let binary = &tool.command;
                if binary.contains('/') {
                    (name, self.root.join(binary).to_string_lossy().into_owned())
                } else {
                    (name, binary.clone())
                }
            })
            .collect()
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
                        directory: Rc::new(self.context.root.join(package_name)),
                        command: command.clone(),
                        silent: *silent,
                    });
                }
                data::Run::Task(task) => self.push(&task.clone().relative_to(package_name))?,
            }
        }

        Ok(())
    }

    pub fn execute(self) -> anyhow::Result<()> {
        let tools = self.context.tool_env();
        let tool_regex = Regex::new(r"(?:^|[^\\])(\{([[:word:]-]+)\})").unwrap();

        for entry in &self.plan {
            if !entry.silent {
                eprintln!("wrun({}): {}", entry.task, entry.command);
            }

            // Recursively replace all instances of {tool}
            let mut command = entry.command.clone();
            let command = loop {
                let mut done = true;
                for capture in tool_regex.captures_iter(&command.clone()) {
                    let range = capture.get(1).unwrap().range();
                    let tool = capture.get(2).unwrap().as_str();

                    if let Some(replacement) = tools.get(tool) {
                        command.replace_range(range, replacement);
                        done = false;
                    }
                }

                if done {
                    break command.as_str();
                }
            };

            let exit = Command::new("sh")
                .current_dir(&*entry.directory)
                .envs(self.context.dotenv()?)
                .envs(tools.iter())
                .args(["-c", command])
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
struct PlanEntry {
    task: AbsoluteTaskName,
    directory: Rc<PathBuf>,
    command: String,
    silent: bool,
}

fn toml_from_path<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}
