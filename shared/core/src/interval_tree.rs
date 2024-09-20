use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval<T> {
    start: T,
    end: T,
}

impl<T: Ord> Interval<T> {
    pub fn new(start: T, end: T) -> Self {
        assert!(start <= end, "Start must be less than or equal to end");
        Interval { start, end }
    }

    pub fn contains(&self, point: T) -> bool {
        self.start <= point && point <= self.end
    }

    pub fn overlaps(&self, other: &Interval<T>) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

#[derive(Debug)]
pub struct IntervalTree<T, V> {
    tree: BTreeMap<T, (Interval<T>, V)>,
}

impl<T: Copy + Ord, V> IntervalTree<T, V> {
    pub fn new() -> Self {
        IntervalTree {
            tree: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, interval: Interval<T>, value: V) -> Result<(), String> {
        if let Some((_, existing_interval)) = self.tree.range(..=interval.start).next_back() {
            if existing_interval.0.end >= interval.start {
                return Err("Overlapping interval".to_string());
            }
        }
        if let Some((_, existing_interval)) = self.tree.range(interval.start..).next() {
            if existing_interval.0.start <= interval.end {
                return Err("Overlapping interval".to_string());
            }
        }
        self.tree.insert(interval.start, (interval, value));
        Ok(())
    }

    pub fn get(&self, point: T) -> Option<&V> {
        self.tree
            .range(..=point)
            .next_back()
            .filter(|(_, (interval, _))| interval.contains(point))
            .map(|(_, (_, value))| value)
    }

    pub fn remove(&mut self, interval: &Interval<T>) -> Option<V> {
        self.tree
            .remove(&interval.start)
            .filter(|(stored_interval, _)| stored_interval == interval)
            .map(|(_, value)| value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Interval<T>, &V)> {
        self.tree
            .values()
            .map(|(interval, value)| (interval, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_new() {
        let interval = Interval::new(1, 5);
        assert_eq!(interval.start, 1);
        assert_eq!(interval.end, 5);
    }

    #[test]
    #[should_panic(expected = "Start must be less than or equal to end")]
    fn test_interval_new_invalid() {
        Interval::new(5, 1);
    }

    #[test]
    fn test_interval_contains() {
        let interval = Interval::new(1, 5);
        assert!(interval.contains(1));
        assert!(interval.contains(3));
        assert!(interval.contains(5));
        assert!(!interval.contains(0));
        assert!(!interval.contains(6));
    }

    #[test]
    fn test_interval_overlaps() {
        let interval1 = Interval::new(1, 5);
        let interval2 = Interval::new(3, 7);
        let interval3 = Interval::new(6, 8);
        
        assert!(interval1.overlaps(&interval2));
        assert!(interval2.overlaps(&interval1));
        assert!(interval2.overlaps(&interval3));
        assert!(!interval1.overlaps(&interval3));
    }

    #[test]
    fn test_interval_tree_insert() {
        let mut tree = IntervalTree::new();
        assert!(tree.insert(Interval::new(1, 5), "A").is_ok());
        assert!(tree.insert(Interval::new(7, 10), "B").is_ok());
        assert!(tree.insert(Interval::new(3, 6), "C").is_err());
    }

    #[test]
    fn test_interval_tree_get() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(1, 5), "A").unwrap();
        tree.insert(Interval::new(7, 10), "B").unwrap();

        assert_eq!(tree.get(3), Some(&"A"));
        assert_eq!(tree.get(8), Some(&"B"));
        assert_eq!(tree.get(6), None);
    }

    #[test]
    fn test_interval_tree_remove() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(1, 5), "A").unwrap();
        tree.insert(Interval::new(7, 10), "B").unwrap();

        assert_eq!(tree.remove(&Interval::new(1, 5)), Some("A"));
        assert_eq!(tree.remove(&Interval::new(1, 5)), None);
        assert_eq!(tree.get(3), None);
        assert_eq!(tree.get(8), Some(&"B"));
    }

    #[test]
    fn test_interval_tree_iter() {
        let mut tree = IntervalTree::new();
        tree.insert(Interval::new(1, 5), "A").unwrap();
        tree.insert(Interval::new(7, 10), "B").unwrap();

        let mut iter = tree.iter();
        assert_eq!(iter.next(), Some((&Interval::new(1, 5), &"A")));
        assert_eq!(iter.next(), Some((&Interval::new(7, 10), &"B")));
        assert_eq!(iter.next(), None);
    }
}