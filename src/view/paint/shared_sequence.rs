//! Immutable blocks with a Vec-compatible copy-on-write editing boundary.
//! Sharing certifies byte ownership, never paint validity or GPU residency.
//! Normal readers iterate/index blocks without flattening; explicit mutation
//! detaches storage so an older validated snapshot cannot change underneath it.
use std::fmt;
use std::ops::{
    Deref, DerefMut, Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo,
    RangeToInclusive,
};
use std::slice::SliceIndex;
use std::sync::{Arc, OnceLock};

pub(crate) struct SharedSequence<T> {
    storage: Storage<T>,
    flattened: OnceLock<Vec<T>>,
}
enum Storage<T> {
    Owned(Vec<T>),
    Shared(Arc<BlockStorage<T>>),
}
// These arrays describe one schedule and always change together. Sharing
// them as one unit avoids three atomic ownership operations on every append
// or clone while preserving the same copy-on-write editing boundary.
#[derive(Clone)]
struct BlockStorage<T> {
    blocks: Vec<Arc<[T]>>,
    ends: Vec<usize>,
    // One word per item makes hot chunk-range validation a direct lookup.
    indices: Vec<usize>,
}

impl<T> Default for SharedSequence<T> {
    fn default() -> Self {
        Self {
            storage: Storage::Owned(Vec::new()),
            flattened: OnceLock::new(),
        }
    }
}
impl<T: Clone> SharedSequence<T> {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn with_shared_capacity(capacity: usize) -> Self {
        Self {
            storage: Storage::Shared(Arc::new(BlockStorage {
                blocks: Vec::with_capacity(capacity),
                ends: Vec::with_capacity(capacity),
                indices: Vec::with_capacity(capacity),
            })),
            flattened: OnceLock::new(),
        }
    }
    pub(crate) fn len(&self) -> usize {
        match &self.storage {
            Storage::Owned(values) => values.len(),
            Storage::Shared(storage) => storage.ends.last().copied().unwrap_or(0),
        }
    }
    pub(crate) fn clear(&mut self) {
        *self = Self::new();
    }
    pub(crate) fn push(&mut self, value: T) {
        match &mut self.storage {
            Storage::Owned(values) => values.push(value),
            Storage::Shared(_) => self.append_shared(vec![value].into()),
        }
    }
    pub(crate) fn insert(&mut self, index: usize, value: T) {
        self.deref_mut().insert(index, value);
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub(crate) fn append_shared(&mut self, block: Arc<[T]>) {
        if block.is_empty() {
            return;
        }
        if matches!(self.storage, Storage::Owned(_)) {
            let Storage::Owned(values) =
                std::mem::replace(&mut self.storage, Storage::Owned(Vec::new()))
            else {
                unreachable!()
            };
            let mut blocks = Vec::new();
            let mut ends = Vec::new();
            if !values.is_empty() {
                ends.push(values.len());
                blocks.push(values.into());
            }
            let indices = vec![0; ends.last().copied().unwrap_or(0)];
            self.storage = Storage::Shared(Arc::new(BlockStorage {
                blocks,
                ends,
                indices,
            }));
        }
        self.flattened.take();
        let Storage::Shared(storage) = &mut self.storage else {
            unreachable!()
        };
        // Snapshots share the complete index and block schedule. An append
        // detaches those arrays only if another reader still holds them.
        let BlockStorage {
            blocks,
            ends,
            indices,
        } = Arc::make_mut(storage);
        ends.push(
            ends.last()
                .copied()
                .unwrap_or(0)
                .checked_add(block.len())
                .expect("sequence length overflow"),
        );
        indices.extend(std::iter::repeat_n(blocks.len(), block.len()));
        blocks.push(block);
    }
    /// Borrow the immutable block containing an item, with its sequence start.
    /// Owned or explicitly edited storage cannot provide an allocation proof.
    pub(crate) fn shared_block_at(&self, index: usize) -> Option<(usize, &Arc<[T]>)> {
        let Storage::Shared(storage) = &self.storage else {
            return None;
        };
        let block = *storage.indices.get(index)?;
        let start = block
            .checked_sub(1)
            .map_or(0, |previous| storage.ends[previous]);
        Some((start, &storage.blocks[block]))
    }

    pub(crate) fn shared_blocks(&self) -> Option<&[Arc<[T]>]> {
        match &self.storage {
            Storage::Shared(storage) => Some(&storage.blocks),
            Storage::Owned(_) => None,
        }
    }

    /// A range reader traverses existing blocks without creating a flat copy.
    pub(crate) fn view(&self, range: Range<usize>) -> Option<SequenceView<'_, T>> {
        (range.start <= range.end && range.end <= self.len()).then_some(SequenceView {
            sequence: self,
            range,
        })
    }

    pub(crate) fn freeze_blocks(&mut self) -> Vec<Arc<[T]>> {
        if matches!(self.storage, Storage::Owned(_)) {
            let Storage::Owned(values) =
                std::mem::replace(&mut self.storage, Storage::Owned(Vec::new()))
            else {
                unreachable!()
            };
            self.append_shared(values.into());
        }
        match &self.storage {
            Storage::Owned(_) => Vec::new(),
            Storage::Shared(storage) => storage.blocks.clone(),
        }
    }
    pub(crate) fn into_blocks(self) -> Vec<Arc<[T]>> {
        match self.storage {
            Storage::Owned(values) if values.is_empty() => Vec::new(),
            Storage::Owned(values) => vec![values.into()],
            Storage::Shared(storage) => Arc::unwrap_or_clone(storage).blocks,
        }
    }
    pub(crate) fn iter<'a>(&'a self) -> Iter<'a, T> {
        let inner = match &self.storage {
            Storage::Owned(values) => IterInner::Owned(values.iter()),
            Storage::Shared(storage) => IterInner::Shared(
                storage
                    .blocks
                    .iter()
                    .map(block_iter::<T> as fn(&'a Arc<[T]>) -> std::slice::Iter<'a, T>)
                    .flatten(),
            ),
        };
        Iter {
            inner,
            remaining: self.len(),
        }
    }
    pub(crate) fn first(&self) -> Option<&T> {
        self.item(0)
    }
    pub(crate) fn last(&self) -> Option<&T> {
        self.len().checked_sub(1).and_then(|index| self.item(index))
    }
    pub(crate) fn as_slice(&self) -> &[T] {
        self.deref().as_slice()
    }
    pub(crate) fn get<I: SequenceIndex<T>>(&self, index: I) -> Option<&I::Output> {
        index.get(self)
    }
    fn item(&self, index: usize) -> Option<&T> {
        match &self.storage {
            Storage::Owned(values) => values.get(index),
            Storage::Shared(storage) => {
                let BlockStorage {
                    blocks,
                    ends,
                    indices,
                } = storage.as_ref();
                let block = *indices.get(index)?;
                let start = block.checked_sub(1).map_or(0, |previous| ends[previous]);
                blocks.get(block)?.get(index - start)
            }
        }
    }
    fn range(&self, range: Range<usize>) -> Option<&[T]> {
        if range.start > range.end || range.end > self.len() {
            return None;
        }
        if range.is_empty() {
            return Some(&[]);
        }
        match &self.storage {
            Storage::Owned(values) => values.get(range),
            Storage::Shared(storage) => {
                let BlockStorage {
                    blocks,
                    ends,
                    indices,
                } = storage.as_ref();
                let block = indices[range.start];
                let start = block.checked_sub(1).map_or(0, |previous| ends[previous]);
                if range.end <= ends[block] {
                    blocks[block].get(range.start - start..range.end - start)
                } else {
                    self.as_slice().get(range)
                }
            }
        }
    }

    /// A reflexive field-equality comparison can skip identical immutable
    /// blocks. This is not a validator: use only to compare already-observed
    /// values, never to infer that shared values are well formed.
    pub(crate) fn equivalent_with(
        &self,
        other: &Self,
        mut equal: impl FnMut(&T, &T) -> bool,
    ) -> bool {
        if self.len() != other.len() {
            return false;
        }
        if let (Storage::Shared(left), Storage::Shared(right)) = (&self.storage, &other.storage) {
            if left.ends == right.ends {
                return left
                    .blocks
                    .iter()
                    .zip(right.blocks.iter())
                    .all(|(left, right)| {
                        Arc::ptr_eq(left, right)
                            || left.iter().zip(right.iter()).all(|(a, b)| equal(a, b))
                    });
            }
        }
        self.iter().zip(other.iter()).all(|(a, b)| equal(a, b))
    }
}

pub(crate) struct SequenceView<'a, T> {
    sequence: &'a SharedSequence<T>,
    range: Range<usize>,
}
/// Own only the immutable blocks intersecting a range. Partial block slices
/// retain their offsets; no whole-frame schedule or unrelated block is held.
pub(crate) struct SequenceSnapshot<T> {
    blocks: Vec<(Arc<[T]>, Range<usize>)>,
    len: usize,
}
impl<T: Clone> SequenceSnapshot<T> {
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.blocks
            .iter()
            .flat_map(|(block, range)| block[range.clone()].iter())
    }
    pub(crate) fn shares_with(&self, current: &SequenceView<'_, T>) -> bool {
        if self.len != current.len() {
            return false;
        }
        let mut index = current.range.start;
        for (block, range) in &self.blocks {
            let Some((start, now)) = current.sequence.shared_block_at(index) else {
                return false;
            };
            if !Arc::ptr_eq(block, now) || range.start != index - start {
                return false;
            }
            index += range.len();
        }
        index == current.range.end
    }
}
impl<T> Clone for SequenceView<'_, T> {
    fn clone(&self) -> Self {
        Self {
            sequence: self.sequence,
            range: self.range.clone(),
        }
    }
}
impl<'a, T: Clone> SequenceView<'a, T> {
    pub(crate) fn snapshot(&self) -> SequenceSnapshot<T> {
        let mut blocks = Vec::new();
        let mut index = self.range.start;
        while index < self.range.end {
            if let Some((start, block)) = self.sequence.shared_block_at(index) {
                let end = self.range.end.min(start + block.len());
                blocks.push((block.clone(), index - start..end - start));
                index = end;
            } else {
                let block: Arc<[T]> = self.iter().cloned().collect::<Vec<_>>().into();
                blocks.push((block, 0..self.len()));
                break;
            }
        }
        SequenceSnapshot {
            blocks,
            len: self.len(),
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.range.len()
    }
    pub(crate) fn first(&self) -> Option<&'a T> {
        self.iter().next()
    }
    pub(crate) fn last(&self) -> Option<&'a T> {
        self.iter().next_back()
    }
    pub(crate) fn iter(
        &self,
    ) -> impl ExactSizeIterator<Item = &'a T> + DoubleEndedIterator + use<'a, T> {
        let sequence = self.sequence;
        self.range
            .clone()
            .map(move |index| sequence.item(index).unwrap())
    }
}
impl<T: Clone> Clone for SharedSequence<T> {
    fn clone(&self) -> Self {
        let storage = match &self.storage {
            Storage::Owned(values) => Storage::Owned(values.clone()),
            Storage::Shared(storage) => Storage::Shared(storage.clone()),
        };
        Self {
            storage,
            flattened: OnceLock::new(),
        }
    }
}
impl<T: Clone> Deref for SharedSequence<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        match &self.storage {
            Storage::Owned(values) => values,
            Storage::Shared(_) => self
                .flattened
                .get_or_init(|| self.iter().cloned().collect()),
        }
    }
}
impl<T: Clone> DerefMut for SharedSequence<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if !matches!(self.storage, Storage::Owned(_)) {
            let values = self
                .flattened
                .take()
                .unwrap_or_else(|| self.iter().cloned().collect());
            self.storage = Storage::Owned(values);
        }
        let Storage::Owned(values) = &mut self.storage else {
            unreachable!()
        };
        values
    }
}
impl<T> From<Vec<T>> for SharedSequence<T> {
    fn from(values: Vec<T>) -> Self {
        Self {
            storage: Storage::Owned(values),
            flattened: OnceLock::new(),
        }
    }
}
impl<T: Clone> From<SharedSequence<T>> for Arc<[T]> {
    fn from(values: SharedSequence<T>) -> Self {
        match values.storage {
            Storage::Owned(values) => values.into(),
            Storage::Shared(storage) if storage.blocks.len() == 1 => storage.blocks[0].clone(),
            Storage::Shared(storage) => storage
                .blocks
                .iter()
                .flat_map(|block| block.iter().cloned())
                .collect::<Vec<_>>()
                .into(),
        }
    }
}
impl<T: Clone> From<Arc<[T]>> for SharedSequence<T> {
    fn from(values: Arc<[T]>) -> Self {
        let mut result = Self::new();
        result.append_shared(values);
        result
    }
}
impl<T> FromIterator<T> for SharedSequence<T> {
    fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
        items.into_iter().collect::<Vec<_>>().into()
    }
}
impl<T: Clone> Extend<T> for SharedSequence<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, items: I) {
        match &mut self.storage {
            Storage::Owned(values) => values.extend(items),
            Storage::Shared(_) => self.append_shared(items.into_iter().collect::<Vec<_>>().into()),
        }
    }
}
impl<T: Clone + fmt::Debug> fmt::Debug for SharedSequence<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T: Clone + PartialEq> PartialEq for SharedSequence<T> {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}
impl<T: Clone + Eq> Eq for SharedSequence<T> {}
impl<T: Clone + PartialEq> PartialEq<Vec<T>> for SharedSequence<T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.iter().eq(other.iter())
    }
}

pub(crate) trait SequenceIndex<T: Clone> {
    type Output: ?Sized;
    fn get(self, sequence: &SharedSequence<T>) -> Option<&Self::Output>;
}
impl<T: Clone> SequenceIndex<T> for usize {
    type Output = T;
    fn get(self, sequence: &SharedSequence<T>) -> Option<&T> {
        sequence.item(self)
    }
}
impl<T: Clone> SequenceIndex<T> for Range<usize> {
    type Output = [T];
    fn get(self, sequence: &SharedSequence<T>) -> Option<&[T]> {
        sequence.range(self)
    }
}
impl<T: Clone> SequenceIndex<T> for RangeFrom<usize> {
    type Output = [T];
    fn get(self, sequence: &SharedSequence<T>) -> Option<&[T]> {
        sequence.range(self.start..sequence.len())
    }
}
impl<T: Clone> SequenceIndex<T> for RangeTo<usize> {
    type Output = [T];
    fn get(self, sequence: &SharedSequence<T>) -> Option<&[T]> {
        sequence.range(0..self.end)
    }
}
impl<T: Clone> SequenceIndex<T> for RangeFull {
    type Output = [T];
    fn get(self, sequence: &SharedSequence<T>) -> Option<&[T]> {
        Some(sequence.as_slice())
    }
}
impl<T: Clone> SequenceIndex<T> for RangeInclusive<usize> {
    type Output = [T];
    fn get(self, sequence: &SharedSequence<T>) -> Option<&[T]> {
        sequence.range(*self.start()..self.end().checked_add(1)?)
    }
}
impl<T: Clone> SequenceIndex<T> for RangeToInclusive<usize> {
    type Output = [T];
    fn get(self, sequence: &SharedSequence<T>) -> Option<&[T]> {
        sequence.range(0..self.end.checked_add(1)?)
    }
}
impl<T: Clone, I: SequenceIndex<T>> Index<I> for SharedSequence<T> {
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output {
        self.get(index).expect("sequence index out of bounds")
    }
}
impl<T: Clone, I> IndexMut<I> for SharedSequence<T>
where
    I: SequenceIndex<T> + SliceIndex<[T], Output = <I as SequenceIndex<T>>::Output>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.deref_mut().as_mut_slice()[index]
    }
}

type BlockIter<'a, T> = std::iter::Flatten<
    std::iter::Map<std::slice::Iter<'a, Arc<[T]>>, fn(&'a Arc<[T]>) -> std::slice::Iter<'a, T>>,
>;
fn block_iter<T>(block: &Arc<[T]>) -> std::slice::Iter<'_, T> {
    block.iter()
}
enum IterInner<'a, T> {
    Owned(std::slice::Iter<'a, T>),
    Shared(BlockIter<'a, T>),
}
pub(crate) struct Iter<'a, T> {
    inner: IterInner<'a, T>,
    remaining: usize,
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        let next = match &mut self.inner {
            IterInner::Owned(iter) => iter.next(),
            IterInner::Shared(iter) => iter.next(),
        };
        if next.is_some() {
            self.remaining -= 1;
        }
        next
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<T> DoubleEndedIterator for Iter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let next = match &mut self.inner {
            IterInner::Owned(iter) => iter.next_back(),
            IterInner::Shared(iter) => iter.next_back(),
        };
        if next.is_some() {
            self.remaining -= 1;
        }
        next
    }
}
impl<T> ExactSizeIterator for Iter<'_, T> {}
impl<T> std::iter::FusedIterator for Iter<'_, T> {}
impl<'a, T: Clone> IntoIterator for &'a SharedSequence<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<T: Clone> IntoIterator for SharedSequence<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        match self.storage {
            Storage::Owned(values) => values.into_iter(),
            Storage::Shared(storage) => storage
                .blocks
                .iter()
                .flat_map(|block| block.iter().cloned())
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }
}

#[cfg(test)]
mod tests;

impl<'a, T: Clone> IntoIterator for &'a mut SharedSequence<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.deref_mut().iter_mut()
    }
}
