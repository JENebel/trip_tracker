/// A fixed-size buffer that can be used to store bytes.
///
/// Data layout is as follows:
/// 
/// .....HxxxxT..........
/// 
/// H: head
/// T: tail
/// x: data
pub struct ByteBuffer<const SIZE: usize> {
    buffer: [u8; SIZE],

    /// The index of the first byte
    head: usize,

    /// The index of the last byte
    tail: usize,
}

impl<const SIZE: usize> ByteBuffer<SIZE> {
    /// Creates a new buffer.
    pub fn new() -> Self {
        Self {
            buffer: [0; SIZE],
            head: 0,
            tail: 0,
        }
    }

    /// Pushes the bytes into the buffer.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer[self.tail..self.tail + bytes.len()].copy_from_slice(bytes);
        self.tail += bytes.len();
    }

    /// Pops the n first bytes from the buffer.
    pub fn pop(&mut self, n: usize) -> &[u8] {
        let res = &self.buffer[self.head..self.head + n];
        self.head += n;
        res
    }

    /// Shifts the buffer back to free up space.
    pub fn shift_back(&mut self) {
        let len = self.len();
        self.buffer.copy_within(self.head..self.tail, 0);
        self.head = 0;
        self.tail = len;
    }

    /// Clears the buffer.
    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
    }

    /// Returns the number of bytes in the buffer.
    pub fn len(&self) -> usize {
        self.tail - self.head
    }

    /// Returns the remaining capacity of the buffer.
    pub fn remaining_capacity(&self) -> usize {
        SIZE - self.len()
    }

    /// Returns a slice with the buffer content.
    pub fn slice(&self) -> &[u8] {
        &self.buffer[self.head..self.tail]
    }

    /// Returns a mutable slice with the free space in the buffer.
    ///
    /// If data is written to this slice, the data should be claimed using the `claim` function.
    pub fn remaining_space_mut(&mut self) -> &mut [u8] {
        &mut self.buffer[self.tail..]
    }

    pub fn claim(&mut self, n: usize) {
        self.tail += n;
    }
}