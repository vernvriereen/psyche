use std::collections::VecDeque;

#[derive(Default)]
pub struct BoundedQueue<T, const U: usize> {
    queue: VecDeque<T>,
}

impl<T, const U: usize> BoundedQueue<T, U> {
    pub fn push(&mut self, item: T) {
        self.queue.push_back(item);
        if self.queue.len() > U {
            self.queue.pop_front();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.queue.iter()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

impl<T, const U: usize> IntoIterator for BoundedQueue<T, U> {
    type Item = T;
    type IntoIter = std::collections::vec_deque::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.queue.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_queue() {
        let queue: BoundedQueue<i32, 4> = BoundedQueue::default();
        assert_eq!(queue.iter().count(), 0);
    }

    #[test]
    fn test_push_within_limit() {
        let mut queue = BoundedQueue::<i32, 4>::default();
        queue.push(1);
        queue.push(2);
        queue.push(3);

        assert_eq!(queue.iter().cloned().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn test_iter() {
        let mut queue = BoundedQueue::<i32, 3>::default();
        queue.push(1);
        queue.push(2);
        queue.push(3);

        let mut iter = queue.iter();
        assert_eq!(iter.next(), Some(&1));
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), Some(&3));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_into_iter() {
        let mut queue = BoundedQueue::<i32, 3>::default();
        queue.push(1);
        queue.push(2);
        queue.push(3);

        let collected: Vec<i32> = queue.into_iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }

    #[test]
    fn test_push_to_full_queue() {
        let mut queue = BoundedQueue::<i32, 3>::default();
        queue.push(1);
        queue.push(2);
        queue.push(3);
        queue.push(4);

        assert_eq!(queue.iter().cloned().collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn test_queue_with_max_len_zero() {
        let mut queue = BoundedQueue::<i32, 0>::default();
        queue.push(1);
        queue.push(2);

        assert_eq!(queue.iter().count(), 0);
    }

    #[test]
    fn test_queue_with_max_len_one() {
        let mut queue = BoundedQueue::<i32, 1>::default();
        queue.push(1);
        queue.push(2);
        queue.push(3);

        assert_eq!(queue.iter().cloned().collect::<Vec<_>>(), vec![3]);
    }
}
