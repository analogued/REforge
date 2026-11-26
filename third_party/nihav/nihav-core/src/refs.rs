// Based on NihAV by Kostya Shishkov, licensed under AGPL-3.0-only.
// REforge changed this implementation in 2026 to use copy-on-write Arc buffers.

//! Reference-counted buffers with copy-on-write access.
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Reference-counted buffer reference.
pub struct NABufferRef<T> {
    data: Arc<T>,
}

impl<T> NABufferRef<T> {
    /// Constructs a new instance of `NABufferRef`.
    pub fn new(val: T) -> Self {
        Self { data: Arc::new(val) }
    }

    /// Reports the number of references for the current allocation.
    pub fn get_num_refs(&self) -> usize {
        Arc::strong_count(&self.data)
    }
}

impl<T: Clone> NABufferRef<T> {
    /// Returns mutable data, copying it if another reference is still live.
    pub fn as_mut(&mut self) -> Option<&mut T> {
        Some(self.deref_mut())
    }
}

impl<T> AsRef<T> for NABufferRef<T> {
    fn as_ref(&self) -> &T {
        &self.data
    }
}

impl<T> Deref for NABufferRef<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.as_ref()
    }
}

impl<T: Clone> DerefMut for NABufferRef<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.data)
    }
}

impl<T> Clone for NABufferRef<T> {
    fn clone(&self) -> Self {
        Self { data: Arc::clone(&self.data) }
    }
}

impl<T: Default> Default for NABufferRef<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::NABufferRef;

    #[test]
    fn writes_preserve_shared_frame() {
        let mut current = NABufferRef::new(vec![0, 1]);
        let previous = current.clone();
        current.as_mut().unwrap()[0] = 2;

        assert_eq!(*previous, [0, 1]);
        assert_eq!(*current, [2, 1]);
    }
}
