// A simple fixed-size ring buffer for orders
pub struct OrderRingBuffer<const N: usize> {
    buf: [Option<super::Order>; N],
    head: usize,
    tail: usize,
    len: usize,
}

impl<const N: usize> OrderRingBuffer<N> {
    pub fn new() -> Self {
        Self {
            buf: [None; N],
            head: 0,
            tail: 0,
            len: 0,
        }
    }
    pub fn push_back(&mut self, order: super::Order) -> bool {
        if self.len == N {
            return false; // full
        }
        self.buf[self.tail] = Some(order);
        self.tail = (self.tail + 1) % N;
        self.len += 1;
        true
    }
    pub fn pop_front(&mut self) -> Option<super::Order> {
        if self.len == 0 {
            return None;
        }
        let order = self.buf[self.head].take();
        self.head = (self.head + 1) % N;
        self.len -= 1;
        order
    }
    pub fn front_mut(&mut self) -> Option<&mut super::Order> {
        if self.len == 0 {
            return None;
        }
        self.buf[self.head].as_mut()
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn len(&self) -> usize {
        self.len
    }
}
