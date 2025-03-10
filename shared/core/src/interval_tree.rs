use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    hash::{Hash, Hasher},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ClosedInterval<T> {
    pub start: T,
    pub end: T,
}

impl<T: Ord> ClosedInterval<T> {
    pub fn new(start: T, end: T) -> Self {
        assert!(start <= end, "Start must be less than or equal to end");
        ClosedInterval { start, end }
    }

    pub fn contains(&self, point: T) -> bool {
        self.start <= point && point <= self.end
    }

    pub fn overlaps(&self, other: &ClosedInterval<T>) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

impl<T: fmt::Display + PartialEq> fmt::Display for ClosedInterval<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start == self.end {
            write!(f, "{}", self.start)
        } else {
            write!(f, "[{}, {}]", self.start, self.end)
        }
    }
}

impl<T: Hash> Hash for ClosedInterval<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.start.hash(state);
        self.end.hash(state);
    }
}

impl<T: Ord> From<(T, T)> for ClosedInterval<T> {
    fn from(value: (T, T)) -> Self {
        Self::new(value.0, value.1)
    }
}

impl<T: Ord + Copy> From<&(T, T)> for ClosedInterval<T> {
    fn from(value: &(T, T)) -> Self {
        Self::new(value.0, value.1)
    }
}

#[derive(Debug)]
pub struct IntervalTree<T, V> {
    tree: BTreeMap<T, (ClosedInterval<T>, V)>,
}

impl<T: fmt::Display + Ord, V: fmt::Display + Eq + std::hash::Hash> fmt::Display
    for IntervalTree<T, V>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.tree.is_empty() {
            return write!(f, "IntervalTree {{}}");
        }

        // Group intervals by value
        let mut value_to_intervals: HashMap<&V, Vec<&ClosedInterval<T>>> = HashMap::new();
        for (interval, value) in self.tree.values() {
            value_to_intervals.entry(value).or_default().push(interval);
        }

        write!(f, "IntervalTree {{ ")?;
        let entries: Vec<_> = value_to_intervals
            .iter()
            .map(|(value, intervals)| {
                let intervals_str: Vec<_> = intervals.iter().map(|i| i.to_string()).collect();
                format!("{}: {}", value, intervals_str.join(", "))
            })
            .collect();
        write!(f, "{}", entries.join(", "))?;
        write!(f, " }}")
    }
}

impl<T: Copy + Ord, V> Default for IntervalTree<T, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Ord, V> IntervalTree<T, V> {
    pub fn new() -> Self {
        IntervalTree {
            tree: BTreeMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.tree.clear();
    }

    pub fn insert(&mut self, interval: ClosedInterval<T>, value: V) -> Result<(), String> {
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

    pub fn remove(&mut self, interval: &ClosedInterval<T>) -> Option<V> {
        self.tree
            .remove(&interval.start)
            .filter(|(stored_interval, _)| stored_interval == interval)
            .map(|(_, value)| value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ClosedInterval<T>, &V)> {
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
        let interval = ClosedInterval::new(1, 5);
        assert_eq!(interval.start, 1);
        assert_eq!(interval.end, 5);
    }

    #[test]
    #[should_panic(expected = "Start must be less than or equal to end")]
    fn test_interval_new_invalid() {
        ClosedInterval::new(5, 1);
    }

    #[test]
    fn test_interval_contains() {
        let interval = ClosedInterval::new(1, 5);
        assert!(interval.contains(1));
        assert!(interval.contains(3));
        assert!(interval.contains(5));
        assert!(!interval.contains(0));
        assert!(!interval.contains(6));
    }

    #[test]
    fn test_interval_overlaps() {
        let interval1 = ClosedInterval::new(1, 5);
        let interval2 = ClosedInterval::new(3, 7);
        let interval3 = ClosedInterval::new(6, 8);

        assert!(interval1.overlaps(&interval2));
        assert!(interval2.overlaps(&interval1));
        assert!(interval2.overlaps(&interval3));
        assert!(!interval1.overlaps(&interval3));
    }

    #[test]
    fn test_interval_tree_insert() {
        let mut tree = IntervalTree::new();
        assert!(tree.insert(ClosedInterval::new(1, 5), "A").is_ok());
        assert!(tree.insert(ClosedInterval::new(7, 10), "B").is_ok());
        assert!(tree.insert(ClosedInterval::new(3, 6), "C").is_err());
    }

    #[test]
    fn test_interval_tree_get() {
        let mut tree = IntervalTree::new();
        tree.insert(ClosedInterval::new(1, 5), "A").unwrap();
        tree.insert(ClosedInterval::new(7, 10), "B").unwrap();

        assert_eq!(tree.get(3), Some(&"A"));
        assert_eq!(tree.get(8), Some(&"B"));
        assert_eq!(tree.get(6), None);
    }

    #[test]
    fn test_interval_tree_remove() {
        let mut tree = IntervalTree::new();
        tree.insert(ClosedInterval::new(1, 5), "A").unwrap();
        tree.insert(ClosedInterval::new(7, 10), "B").unwrap();

        assert_eq!(tree.remove(&ClosedInterval::new(1, 5)), Some("A"));
        assert_eq!(tree.remove(&ClosedInterval::new(1, 5)), None);
        assert_eq!(tree.get(3), None);
        assert_eq!(tree.get(8), Some(&"B"));
    }

    #[test]
    fn test_interval_tree_iter() {
        let mut tree = IntervalTree::new();
        tree.insert(ClosedInterval::new(1, 5), "A").unwrap();
        tree.insert(ClosedInterval::new(7, 10), "B").unwrap();

        let mut iter = tree.iter();
        assert_eq!(iter.next(), Some((&ClosedInterval::new(1, 5), &"A")));
        assert_eq!(iter.next(), Some((&ClosedInterval::new(7, 10), &"B")));
        assert_eq!(iter.next(), None);
    }
}
