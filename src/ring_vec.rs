use std::prelude::v1::*;

pub struct RingVec<T> {
    inner: Vec<Option<T>>,
    start: usize,
    end: usize,
}

impl<T: Clone + PartialEq> RingVec<T> {
    pub fn new(n: usize) -> RingVec<T> {
        let inner = vec![None; n];
        Self {
            inner,
            start: 0,
            end: 0,
        }
    }

    pub fn contains(&self, item: &T) -> bool {
        let item = Some(item);
        self.inner.iter().any(|n| n.as_ref() == item)
    }

    pub fn push(&mut self, item: T) {
        let len = self.inner.len();
        self.inner[self.end % len] = Some(item);
        self.end = (self.end + 1) % self.inner.len();
        if self.start == self.end {
            self.start += 1;
        }
    }

    pub fn get(&self, idx: usize) -> Option<&T> {
        let len = self.end - self.start;
        if idx >= len {
            return None;
        }
        let item = &self.inner[(idx + len) % self.inner.len()];
        item.as_ref()
    }
}