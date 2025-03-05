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
