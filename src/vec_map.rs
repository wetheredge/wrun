use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[repr(transparent)]
pub(crate) struct VecMap<T: for<'a> Deserialize<'a> + Serialize>(
    #[serde_as(as = "serde_with::Map<_, _>")] Vec<(String, T)>,
);

impl<T: for<'de> Deserialize<'de> + Serialize> VecMap<T> {
    pub(crate) fn get<'a>(&'a self, key: &str) -> Option<&'a T> {
        self.iter()
            .find_map(|entry| (entry.0 == key).then_some(entry.1))
    }

    pub(crate) fn get_index(&self, key: &str) -> Option<usize> {
        self.iter().position(|entry| entry.0 == key)
    }

    pub(crate) fn get_by_index(&self, index: usize) -> Option<&T> {
        self.0.get(index).map(|entry| &entry.1)
    }

    pub(crate) fn insert(&mut self, key: String, value: T) -> &mut T {
        self.0.push((key, value));
        &mut self.0.last_mut().unwrap().1
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }
}

impl<T: for<'de> Deserialize<'de> + Serialize> Default for VecMap<T> {
    fn default() -> Self {
        Self(Vec::default())
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

impl<T: for<'de> Deserialize<'de> + Serialize> FromIterator<(String, T)> for VecMap<T> {
    fn from_iter<I: IntoIterator<Item = (String, T)>>(iter: I) -> Self {
        Self(Vec::from_iter(iter))
    }
}
