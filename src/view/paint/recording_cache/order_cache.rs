use super::*;

impl RecordingCache {
    /// Share path storage only after comparing the current canonical walk.
    /// This does not reuse traversal, root index, phase, slot or owner validity.
    /// Old manifests retain their immutable path when a later observation moves
    /// an owner, so metadata/full order drift is still visible. Unseen paths are
    /// released at the same accepted-frame boundary as command entries.
    pub(in super::super) fn intern_order_path(&mut self, owner: NodeKey, path: &[usize]) -> Arc<[usize]> {
        if let Some((previous, seen)) = self.order_paths.get_mut(&owner) {
            if previous.as_ref() == path {
                *seen = true;
                return previous.clone();
            }
        }
        let path: Arc<[usize]> = path.into();
        self.order_paths.insert(owner, (path.clone(), true));
        path
    }
}

#[cfg(test)]
mod tests;
