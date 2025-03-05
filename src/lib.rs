mod data;
mod vec_map;

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::bail;

pub use self::data::TaskName;
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

    pub fn local_tasks(&self) -> &data::Tasks {
        let package = self.local.as_deref().unwrap_or("");
        let package = self.packages.get(package).unwrap();
        &package.tasks
    }

    pub fn plan(&mut self) -> Plan {
        Plan {
            context: self,
            plan: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Plan<'a> {
    context: &'a mut Context,
    plan: Vec<Command>,
}

impl Plan<'_> {
    pub fn push(&mut self, task: &TaskName) -> anyhow::Result<()> {
        let package = task
            .package()
            .unwrap_or_else(|| self.context.local_package_name())
            .to_owned();
        let package = self.context.get_package(&package)?;

        let Some(task) = package.tasks.0.get(task.task()) else {
            bail!("Cannot find task: {task}")
        };
        let task = Rc::clone(task);

        for run in &task.run {
            match run {
                data::Run::Command { command, silent } => {
                    self.plan.push(Command {
                        command: command.clone(),
                        silent: *silent,
                    });
                }
                data::Run::Task(task) => self.push(task)?,
            }
        }

        Ok(())
    }

    pub fn execute(self) -> anyhow::Result<()> {
        for Command { command, .. } in &self.plan {
            dbg!(command);
        }

        Ok(())
    }
}

#[derive(Debug)]
struct Command {
    command: String,
    silent: bool,
}

fn toml_from_path<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}
