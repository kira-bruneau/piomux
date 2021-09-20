use std::{
    cmp::min,
    fmt::Debug,
    intrinsics::transmute,
    mem::MaybeUninit,
    ops::{Add, AddAssign},
    ptr::slice_from_raw_parts_mut,
};

// This was manually implemented because I couldn't find an existing ring buffer library with the following requirements:
// - Provides a way to get mutable slices to unused sections so it can be directly written into
// - Provides an interface for multiple readers (eg. a way to read from the buffer using multiple external cursors)
// - Designed for single threaded use (lock-free, non-atomic)
// - Fixed width
// - No dynamic memory allocation
// - Doesn't require initialization
#[derive(Debug)]
pub struct RingBuffer<T, const CAPACITY: usize> {
    data: [MaybeUninit<T>; CAPACITY],
    cursor: RingBufferCursor<CAPACITY>,
    len: usize,
}

impl<T, const CAPACITY: usize> RingBuffer<T, CAPACITY> {
    pub fn new() -> Self {
        Self {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            cursor: RingBufferCursor { inner: 0 },
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == CAPACITY
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn start(&self) -> RingBufferCursor<CAPACITY> {
        self.cursor
    }

    pub fn end(&self) -> RingBufferCursor<CAPACITY> {
        self.cursor + self.len
    }

    pub fn unused_slices(&mut self) -> [&mut [MaybeUninit<T>]; 2] {
        let start = self.start().inner;
        let end = self.end().inner;
        if self.is_empty() || end > start {
            unsafe {
                // To grab two mutable references from a single mutable reference, we have to use unsafe code. This won't be a problem since first & second won't overlap.
                let first = &mut self.data[end..];
                let first = &mut *slice_from_raw_parts_mut(first.as_mut_ptr(), first.len());
                let second = &mut self.data[..start];
                let second = &mut *slice_from_raw_parts_mut(second.as_mut_ptr(), second.len());
                [first, second]
            }
        } else {
            [&mut self.data[end..start], &mut []]
        }
    }

    pub fn slices_from(&self, cursor: RingBufferCursor<CAPACITY>) -> [&[u8]; 2] {
        // The unsafe blocks assume that anything between start & end has already been initialized.
        let cursor = cursor.inner;
        let start = self.start().inner;
        let end = self.end().inner;
        if self.is_empty() || end > start {
            if cursor >= start && cursor < end {
                unsafe { [transmute(&self.data[cursor..end]), &[]] }
            } else {
                [&[], &[]]
            }
        } else {
            if cursor >= start {
                unsafe {
                    [
                        transmute(&self.data[cursor..]),
                        transmute(&self.data[..end]),
                    ]
                }
            } else if cursor < end {
                unsafe { [transmute(&self.data[cursor..end]), &[]] }
            } else {
                [&[], &[]]
            }
        }
    }

    pub unsafe fn assume_init(&mut self, n: usize) {
        self.len = min(self.len + n, CAPACITY);
    }

    pub fn remove(&mut self, n: usize) {
        let n = min(n, self.len);
        self.cursor += n;
        self.len -= n;
    }
}

#[derive(Clone, Copy)]
pub struct RingBufferCursor<const CAPACITY: usize> {
    inner: usize,
}

impl<const CAPACITY: usize> Add<usize> for RingBufferCursor<CAPACITY> {
    type Output = RingBufferCursor<CAPACITY>;

    fn add(self, rhs: usize) -> Self::Output {
        Self::Output {
            inner: (self.inner + rhs) % CAPACITY,
        }
    }
}

impl<const CAPACITY: usize> AddAssign<usize> for RingBufferCursor<CAPACITY> {
    fn add_assign(&mut self, rhs: usize) {
        *self = *self + rhs;
    }
}

impl<const CAPACITY: usize> Debug for RingBufferCursor<CAPACITY> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

#[cfg(test)]
mod test {
    use super::{RingBuffer, RingBufferCursor};

    #[test]
    fn unused_slices_contiguous_when_empty() {
        let mut buffer = RingBuffer::<u8, 8>::new();
        let ptr = buffer.data.as_ptr() as isize;

        let [first, second] = buffer.unused_slices();
        assert_eq!(first.as_ptr() as isize - ptr, 0);
        assert_eq!(first.len(), 8);
        assert_eq!(second.len(), 0);
    }

    #[test]
    fn unused_slices_noncontiguous_when_empty() {
        let mut buffer = RingBuffer::<u8, 8>::new();
        let ptr = buffer.data.as_ptr() as isize;
        buffer.cursor = RingBufferCursor { inner: 3 };

        let [first, second] = buffer.unused_slices();
        assert_eq!(first.as_ptr() as isize - (ptr + 3), 0);
        assert_eq!(first.len(), 5);
        assert_eq!(second.as_ptr() as isize - ptr, 0);
        assert_eq!(second.len(), 3);
    }

    #[test]
    fn unused_slices_contiguous_when_partially_filled() {
        let mut buffer = RingBuffer::<u8, 8>::new();
        let ptr = buffer.data.as_ptr() as isize;
        buffer.cursor = RingBufferCursor { inner: 6 };
        buffer.len = 3;

        let [first, second] = buffer.unused_slices();
        assert_eq!(first.as_ptr() as isize - (ptr + 1), 0);
        assert_eq!(first.len(), 5);
        assert_eq!(second.len(), 0);
    }

    #[test]
    fn unused_slices_noncontiguous_when_partially_filled() {
        let mut buffer = RingBuffer::<u8, 8>::new();
        let ptr = buffer.data.as_ptr() as isize;
        buffer.cursor = RingBufferCursor { inner: 2 };
        buffer.len = 5;

        let [first, second] = buffer.unused_slices();
        assert_eq!(first.as_ptr() as isize - (ptr + 7), 0);
        assert_eq!(first.len(), 1);
        assert_eq!(second.as_ptr() as isize - ptr, 0);
        assert_eq!(second.len(), 2);
    }

    #[test]
    fn unused_slices_when_full() {
        let mut buffer = RingBuffer::<u8, 8>::new();
        buffer.len = 8;

        let [first, second] = buffer.unused_slices();
        assert_eq!(first.len(), 0);
        assert_eq!(second.len(), 0);
    }

    #[test]
    fn slices_from_when_empty() {
        let buffer = RingBuffer::<u8, 8>::new();

        let [first, second] = buffer.slices_from(RingBufferCursor { inner: 7 });
        assert_eq!(first.len(), 0);
        assert_eq!(second.len(), 0);
    }

    #[test]
    fn slices_from_contiguous_when_partially_filled() {
        let mut buffer = RingBuffer::<u8, 8>::new();
        unsafe {
            *buffer.data[2].as_mut_ptr() = 0;
            *buffer.data[3].as_mut_ptr() = 1;
        }

        buffer.cursor = RingBufferCursor { inner: 2 };
        buffer.len = 2;

        let [first, second] = buffer.slices_from(RingBufferCursor { inner: 3 });
        assert_eq!(first, &[1]);
        assert_eq!(second, &[]);
    }

    #[test]
    fn slices_from_noncontiguous_when_partially_filled() {
        let mut buffer = RingBuffer::<u8, 8>::new();
        unsafe {
            *buffer.data[5].as_mut_ptr() = 0;
            *buffer.data[6].as_mut_ptr() = 1;
            *buffer.data[7].as_mut_ptr() = 2;
            *buffer.data[0].as_mut_ptr() = 3;
            *buffer.data[1].as_mut_ptr() = 4;
        }

        buffer.cursor = RingBufferCursor { inner: 5 };
        buffer.len = 5;

        let [first, second] = buffer.slices_from(RingBufferCursor { inner: 7 });
        assert_eq!(first, &[2]);
        assert_eq!(second, &[3, 4]);
    }

    // #[test]
    // fn slices_from_when_full() {
    //     let mut buffer = RingBuffer::<u8, 8>::new();
    //     buffer.len = 8;

    //     let [first, second] = buffer.slices_from();
    //     assert_eq!(first.len(), 0);
    //     assert_eq!(second.len(), 0);
    // }
}
