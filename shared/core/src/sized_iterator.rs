pub struct SizedIterator<I> {
    iter: I,
    size: usize,
}

impl<I> SizedIterator<I> {
    pub fn new(iter: I, size: usize) -> Self {
        Self { iter, size }
    }
}

impl<I: Iterator> Iterator for SizedIterator<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }
}

impl<I: Iterator> ExactSizeIterator for SizedIterator<I> {}
