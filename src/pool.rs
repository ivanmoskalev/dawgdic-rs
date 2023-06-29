use crate::unit::BaseType;
use std::convert::TryFrom;
use std::ops::{Index, IndexMut};

#[derive(Clone)]
pub struct Pool<T: Clone> {
    inner: Vec<T>,
}

impl<T: Clone> Default for Pool<T> {
    fn default() -> Pool<T> {
        Pool { inner: Vec::new() }
    }
}

impl<T: Clone> Pool<T> {
    pub fn from_vec(inner: Vec<T>) -> Pool<T> {
        Pool { inner }
    }

    pub fn get(&self, index: BaseType) -> Option<&T> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.inner.get(index))
    }

    pub fn push(&mut self, value: T) {
        self.inner.push(value)
    }

    pub fn len(&self) -> BaseType {
        BaseType::try_from(self.inner.len()).unwrap()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&mut self) {
        self.inner.clear()
    }

    pub fn resize(&mut self, size: u32, value: T) {
        self.inner.resize(usize::try_from(size).unwrap(), value)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.inner.iter()
    }
}

impl<T: Clone> Index<BaseType> for Pool<T> {
    type Output = T;
    fn index(&self, index: BaseType) -> &T {
        let index = usize::try_from(index).unwrap();
        &self.inner[index]
    }
}

impl<T: Clone> IndexMut<BaseType> for Pool<T> {
    fn index_mut(&mut self, index: BaseType) -> &mut T {
        let index = usize::try_from(index).unwrap();
        &mut self.inner[index]
    }
}
