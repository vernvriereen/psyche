use anchor_lang::{prelude::borsh, AnchorDeserialize, AnchorSerialize, InitSpace, Space};
use bytemuck::Zeroable;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Copy, Zeroable, InitSpace, AnchorSerialize, AnchorDeserialize, Serialize, Deserialize)]
#[serde(bound = "T: Serialize + DeserializeOwned")]
pub struct FixedVec<T, const N: usize> where T: Space {
    #[serde(with = "serde_arrays")]
    data: [T; N],
    len: u64,
}

impl<T: Default + Copy + Space, const N: usize> FixedVec<T, N> {
    /// Creates a new empty FixedVec
    pub fn new() -> Self {
        Self {
            data: [T::default(); N],
            len: 0,
        }
    }

    /// Returns the number of elements in the vector
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns true if the vector contains no elements
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns true if the vector is at capacity
    pub fn is_full(&self) -> bool {
        self.len as usize == N
    }

    /// Returns the total capacity of the vector
    pub fn capacity(&self) -> usize {
        N
    }

    /// Pushes an element to the back of the vector
    /// Returns Err if the vector is full
    pub fn push(&mut self, value: T) -> Result<(), &'static str> {
        if (self.len as usize) < N {
            self.data[self.len as usize] = value;
            self.len += 1;
            Ok(())
        } else {
            Err("FixedVec is full")
        }
    }

    /// Removes and returns the last element
    pub fn pop(&mut self) -> Option<T> {
        if self.len > 0 {
            self.len -= 1;
            Some(self.data[self.len as usize])
        } else {
            None
        }
    }

    /// Returns a reference to an element at the given index
    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len as usize {
            Some(&self.data[index])
        } else {
            None
        }
    }

    /// Returns a mutable reference to an element at the given index
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index < self.len as usize {
            Some(&mut self.data[index])
        } else {
            None
        }
    }

    /// Clears the vector, removing all elements
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Returns an iterator over the vector
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data[0..self.len as usize].iter()
    }

    /// Returns a mutable iterator over the vector
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.data[0..self.len as usize].iter_mut()
    }

    /// Extends the vector with elements from an iterator
    pub fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) -> Result<(), &'static str> {
        for item in iter {
            self.push(item)?;
        }
        Ok(())
    }

    /// Returns a reference to the first element
    pub fn first(&self) -> Option<&T> {
        if self.len > 0 {
            Some(&self.data[0])
        } else {
            None
        }
    }

    /// Returns a reference to the last element
    pub fn last(&self) -> Option<&T> {
        if self.len > 0 {
            Some(&self.data[self.len as usize - 1])
        } else {
            None
        }
    }

    /// Removes an element at the given index
    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index >= self.len as usize {
            return None;
        }

        let item = Some(self.data[index]);

        // Shift remaining elements
        for i in index..self.len as usize - 1 {
            self.data[i] = self.data[i + 1];
        }

        self.len -= 1;
        item
    }

    /// Inserts an element at the given index
    pub fn insert(&mut self, index: usize, element: T) -> Result<(), &'static str> {
        if self.len as usize >= N {
            return Err("FixedVec is full");
        }
        if index > self.len as usize {
            return Err("Index out of bounds");
        }

        // Shift elements to make space
        for i in (index..self.len as usize).rev() {
            self.data[i + 1] = self.data[i];
        }

        self.data[index] = element;
        self.len += 1;
        Ok(())
    }
}

// Implement Index trait for convenient indexing
impl<T: Default + Copy + Space, const N: usize> std::ops::Index<usize> for FixedVec<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("Index out of bounds")
    }
}

// Implement IndexMut trait for mutable indexing
impl<T: Default + Copy + Space, const N: usize> std::ops::IndexMut<usize> for FixedVec<T, N> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("Index out of bounds")
    }
}

impl<T: Default + Copy + Space, const N: usize> Default for FixedVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
