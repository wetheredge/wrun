mod data;
mod vec_map;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::bail;

use self::vec_map::VecMap;

const PROJECT_FILE: &str = "wrun-project.toml";
const PACKAGE_FILE: &str = "wrun.toml";

#[derive(Debug)]
pub struct Context {
    root: PathBuf,
    project: data::Project,
    local: Option<(String, data::Package)>,
    others: HashMap<String, data::Package>,
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

        let project = toml_from_path(&root.join(PROJECT_FILE))?;
        let mut context = Self {
            root,
            project,
            local: None,
            others: HashMap::new(),
        };

        if let Some(path) = package_dir {
            let relative = path.strip_prefix(&context.root).unwrap();
            let name = relative.to_string_lossy().into_owned();
            let package = context.load_package(relative)?;
            context.local = Some((name, package));
        }

        Ok(context)
    }

    fn load_package(&self, path: &Path) -> anyhow::Result<data::Package> {
        toml_from_path(&self.root.join(path).join(PACKAGE_FILE))
    }

    fn get_package(&mut self, name: &str) -> anyhow::Result<&data::Package> {
        if let Some((local, package)) = &self.local {
            if local == name {
                return Ok(package);
            }
        }

        if !self.others.contains_key(name) {
            let package = self.load_package(Path::new(name))?;
            self.others.insert(name.to_owned(), package);
        }

        Ok(self.others.get(name).unwrap())
    }

    pub fn local_tasks(&self) -> &data::Tasks {
        if let Some((_, package)) = &self.local {
            &package.tasks
        } else {
            &self.project.package.tasks
        }
    }

    pub fn run(&mut self, task: &str) -> anyhow::Result<()> {
        let (package, task) = if let Some((package, task)) = task.split_once('/') {
            (self.get_package(package)?, task)
        } else if let Some((_, package)) = self.local.as_ref() {
            (package, task)
        } else {
            (&self.project.package, task)
        };

        dbg!(task, package);

        Ok(())
    }
}

fn toml_from_path<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}
