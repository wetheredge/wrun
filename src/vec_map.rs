use std::collections::HashMap;
use std::marker::PhantomData;

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize};
use serde_with::serde_as;

#[serde_as]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub(crate) struct VecMap<T>(Vec<(String, T)>);

impl<T> VecMap<T> {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

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

impl<T> Default for VecMap<T> {
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<T: PartialEq> PartialEq for VecMap<T> {
    fn eq(&self, other: &Self) -> bool {
        type Map<'a, T> = HashMap<&'a str, &'a T>;
        let lhs = self.iter().collect::<Map<T>>();
        let rhs = other.iter().collect::<Map<T>>();
        lhs.eq(&rhs)
    }
}

impl<T> FromIterator<(String, T)> for VecMap<T> {
    fn from_iter<I: IntoIterator<Item = (String, T)>>(iter: I) -> Self {
        Self(Vec::from_iter(iter))
    }
}

impl<T> From<Vec<(String, T)>> for VecMap<T> {
    fn from(inner: Vec<(String, T)>) -> Self {
        Self(inner)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for VecMap<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de;

        struct VecMapVisitor<T>(PhantomData<fn() -> VecMap<T>>);

        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for VecMapVisitor<T> {
            type Value = VecMap<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: de::MapAccess<'de>,
            {
                let mut map =
                    VecMap::from(Vec::with_capacity(access.size_hint().unwrap_or_default()));

                while let Some((key, value)) = access.next_entry()? {
                    map.insert(key, value);
                }

                Ok(map)
            }
        }

        deserializer.deserialize_map(VecMapVisitor(PhantomData))
    }
}

impl<T: Serialize> Serialize for VecMap<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self.iter() {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
