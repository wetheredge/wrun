use std::fs;
use std::path::{Path, PathBuf};

use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

const PROJECT_FILE: &str = "wrun-project.toml";
const PACKAGE_FILE: &str = "wrun.toml";

#[derive(Debug)]
pub struct Context {
    project: Project,
    package: Option<Package>,
}

impl Context {
    pub fn from_directory(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut package = None::<PathBuf>;
        let mut project = None;

        for dir in dir.as_ref().ancestors() {
            let try_package = dir.join(PACKAGE_FILE);
            if fs::exists(&try_package)? {
                if let Some(package) = package {
                    bail!(
                        "Packages cannot be nested: {} is inside {}",
                        package.to_string_lossy(),
                        try_package.to_string_lossy()
                    );
                } else {
                    package = Some(try_package);
                }
            }

            let try_project = dir.join(PROJECT_FILE);
            if fs::exists(&try_project)? {
                project = Some(try_project);
                break;
            }
        }
        let Some(project) = project else {
            bail!("failed to find project file")
        };

        let project = toml_from_path(&project)?;
        let package = package.as_deref().map(toml_from_path).transpose()?;

        Ok(Self { project, package })
    }

    pub fn local_tasks(&self) -> &Tasks {
        if let Some(package) = &self.package {
            &package.tasks
        } else {
            &self.project.tasks
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Project {
    pub env_files: Vec<String>,
    pub tasks: Tasks,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Package {
    pub tasks: Tasks,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[repr(transparent)]
pub struct Tasks(#[serde_as(as = "serde_with::Map<_, _>")] Vec<(String, Task)>);

impl Tasks {
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Task)> {
        self.0.iter().map(|(name, task)| (name.as_str(), task))
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Task {
    #[serde(alias = "desc", skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "skip_false")]
    internal: bool,
    #[serde_as(as = "serde_with::OneOrMany<_>")]
    run: Vec<Run>,
}

impl Task {
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn is_internal(&self) -> bool {
        self.internal
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub enum Run {
    Command { command: String, silent: bool },
    Task(String),
}

impl<'de> Deserialize<'de> for Run {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Error};

        struct RunVisitor;

        impl<'de> de::Visitor<'de> for RunVisitor {
            type Value = Run;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("string or map")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let mut command = v;
                let mut silent = false;
                if let Some(s) = command.strip_prefix('@') {
                    command = s;
                    silent = true;
                }

                let command = command.to_owned();
                Ok(Run::Command { command, silent })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                enum Variant {
                    Unknown,
                    Command,
                    Task,
                }

                impl Variant {
                    fn could_be(self, other: Self) -> bool {
                        self == Self::Unknown || self == other
                    }
                }

                let mut variant = Variant::Unknown;
                let mut command = None;
                let mut silent = None;
                let mut task = None;

                while let Some(key) = map.next_key::<String>()? {
                    let key = key.as_str();

                    if variant.could_be(Variant::Command) {
                        match key {
                            "command" | "cmd" => {
                                if command.is_some() {
                                    return Err(Error::duplicate_field("command"));
                                }
                                command = Some(map.next_value()?);
                                variant = Variant::Command;
                                continue;
                            }
                            "silent" => {
                                if silent.is_some() {
                                    return Err(Error::duplicate_field("silent"));
                                }
                                silent = Some(map.next_value()?);
                                variant = Variant::Command;
                                continue;
                            }
                            _ => {}
                        }
                    }

                    if variant.could_be(Variant::Task) && key == "task" {
                        if task.is_some() {
                            return Err(Error::duplicate_field("task"));
                        }
                        task = Some(map.next_value()?);
                        variant = Variant::Task;
                        continue;
                    }

                    return Err(Error::unknown_field(key, &["command", "silent", "task"]));
                }

                if let Some(command) = command {
                    let silent = silent.unwrap_or_default();
                    Ok(Run::Command { command, silent })
                } else if let Some(task) = task {
                    Ok(Run::Task(task))
                } else {
                    Err(Error::missing_field("command or task"))
                }
            }
        }

        deserializer.deserialize_any(RunVisitor)
    }
}

fn skip_false(b: &bool) -> bool {
    !*b
}

fn toml_from_path<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
    struct Wrapper<T> {
        test: T,
    }

    fn command(command: &str, silent: bool) -> Run {
        Run::Command {
            command: command.to_owned(),
            silent,
        }
    }

    macro_rules! toml_eq {
        ($expected:expr, $toml:expr) => {
            assert_eq!(Wrapper { test: $expected }, toml::from_str($toml).unwrap());
        };
    }

    #[test]
    fn run_command_shorthand() {
        toml_eq!(command("echo loud", false), r#"test = "echo loud""#);
        toml_eq!(command("echo silent", true), r#"test = "@echo silent""#);
    }

    #[test]
    fn run_command() {
        toml_eq!(command("foo", false), r#"test = { cmd = "foo" }"#);
        toml_eq!(
            command("bar", true),
            r#"test = { command = "bar", silent = true }"#
        );
    }

    #[test]
    fn run_task() {
        toml_eq!(Run::Task("quux".to_owned()), r#"test = { task = "quux" }"#);
    }

    #[test]
    fn task_run_single() {
        let task = Task {
            desc: None,
            run: vec![command("echo test", true)],
        };
        toml_eq!(task, r#"test = { run = "@echo test" }"#);
    }

    #[test]
    fn task_run_multiple() {
        let task = Task {
            desc: None,
            run: vec![command("one", false), command("two", false)],
        };
        toml_eq!(task, r#"test = { run = ["one", "two"] }"#);
    }
}
