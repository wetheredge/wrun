use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Project {
    env_files: Vec<String>,
    tools: VecMap<Tool>,
    #[serde(flatten)]
    pub(crate) package: Package,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct Tool {
    #[serde(alias = "bin")]
    binary: String,
    ci: Option<ToolCi>,
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
enum ToolCi {
    Action {
        action: String,
        #[serde(default)]
        with: VecMap<String>,
    },
    #[serde(rename_all = "kebab-case")]
    Binary {
        install_action: String,
        #[serde(default)]
        with: VecMap<String>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Package {
    pub(crate) tasks: Tasks,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
#[repr(transparent)]
pub struct Tasks(VecMap<Task>);

impl Tasks {
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Task)> {
        self.0.iter()
    }
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Task {
    #[serde(default, skip_serializing_if = "skip_false")]
    internal: bool,
    #[serde(alias = "desc", skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, alias = "deps")]
    dependencies: Vec<TaskName>,
    #[serde(default)]
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

#[derive(Debug, Clone, PartialEq, serde_with::SerializeDisplay, serde_with::DeserializeFromStr)]
pub enum TaskName {
    Local(String),
    Root(String),
    Qualified { package: String, task: String },
}

impl FromStr for TaskName {
    type Err = Never;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((package, task)) = s.rsplit_once('/') {
            let task = task.to_owned();
            if package.is_empty() {
                Ok(Self::Root(task))
            } else {
                let package = package.to_owned();
                Ok(Self::Qualified { package, task })
            }
        } else {
            Ok(Self::Local(s.to_owned()))
        }
    }
}

impl fmt::Display for TaskName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskName::Local(task) => f.write_str(task),
            TaskName::Root(task) => {
                f.write_str("/")?;
                f.write_str(task)
            }
            TaskName::Qualified { package, task } => {
                f.write_str(package)?;
                f.write_str("/")?;
                f.write_str(task)
            }
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub enum Run {
    Command { command: String, silent: bool },
    Task(TaskName),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[doc(hidden)]
pub struct Never {}

impl fmt::Display for Never {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}

#[serde_as]
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[repr(transparent)]
struct VecMap<T: for<'a> Deserialize<'a> + Serialize>(
    #[serde_as(as = "serde_with::Map<_, _>")] Vec<(String, T)>,
);

impl<T: for<'de> Deserialize<'de> + Serialize> VecMap<T> {
    fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }
}

impl<T: for<'de> Deserialize<'de> + Serialize + PartialEq> PartialEq for VecMap<T> {
    fn eq(&self, other: &Self) -> bool {
        type Map<'a, T> = HashMap<&'a str, &'a T>;
        let lhs = self.iter().collect::<Map<T>>();
        let rhs = other.iter().collect::<Map<T>>();
        lhs.eq(&rhs)
    }
}

fn skip_false(b: &bool) -> bool {
    !*b
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

    macro_rules! task {
        ($package:literal / $task:literal) => {
            TaskName::Qualified {
                package: $package.to_owned(),
                task: $task.to_owned(),
            }
        };
        (/ $task:literal) => {
            TaskName::Root($task.to_owned())
        };
        ($task:literal) => {
            TaskName::Local($task.to_owned())
        };
    }

    macro_rules! toml_eq {
        ($expected:expr, $toml:expr) => {
            assert_eq!(Wrapper { test: $expected }, toml::from_str($toml).unwrap());
        };
    }

    #[test]
    fn deep_task_name() {
        toml_eq!(task!("foo/bar" / "baz"), r#"test = "foo/bar/baz""#);
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
    fn run_task_local() {
        toml_eq!(Run::Task(task!("local")), r#"test = { task = "local" }"#);
    }

    #[test]
    fn run_task_root() {
        toml_eq!(Run::Task(task!(/ "root")), r#"test = { task = "/root" }"#);
    }

    #[test]
    fn run_task_qualified() {
        toml_eq!(
            Run::Task(task!("fully" / "qualified")),
            r#"test = { task = "fully/qualified" }"#
        );
    }

    #[test]
    fn task_run_single() {
        let task = Task {
            internal: false,
            description: None,
            dependencies: vec![],
            run: vec![command("echo test", true)],
        };
        toml_eq!(task, r#"test = { run = "@echo test" }"#);
    }

    #[test]
    fn task_run_multiple() {
        let task = Task {
            internal: false,
            description: None,
            dependencies: vec![],
            run: vec![command("one", false), command("two", false)],
        };
        toml_eq!(task, r#"test = { run = ["one", "two"] }"#);
    }

    #[test]
    fn task_dependencies() {
        let task = Task {
            internal: false,
            description: None,
            dependencies: vec![task!("local"), task!(/ "root"), task!("some" / "other")],
            run: vec![],
        };
        toml_eq!(task, r#"test.deps = ["local", "/root", "some/other"]"#);
    }

    #[test]
    fn tool_simple() {
        let tool = Tool {
            binary: "tool".to_owned(),
            ci: None,
        };
        toml_eq!(tool, r#"test.bin = "tool""#);
    }

    #[test]
    fn tool_ci_action() {
        let tool = Tool {
            binary: "tool".to_owned(),
            ci: Some(ToolCi::Action {
                action: "ci/action".to_owned(),
                with: VecMap(vec![("some".to_owned(), "thing".to_owned())]),
            }),
        };
        toml_eq!(
            tool,
            r#"test = { bin = "tool", ci.action = "ci/action", ci.with.some = "thing" }"#
        );
    }

    #[test]
    fn tool_ci_binary() {
        let tool = Tool {
            binary: "tool".to_owned(),
            ci: Some(ToolCi::Binary {
                install_action: "install/action".to_owned(),
                with: VecMap::default(),
            }),
        };
        toml_eq!(
            tool,
            r#"test = { bin = "tool", ci.install-action = "install/action" }"#
        );
    }
}
