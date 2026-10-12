#[cfg(test)]
thread_local! {
    static FULL_ARTIFACT_RECORDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn take_full_artifact_record_count() -> usize {
    FULL_ARTIFACT_RECORDS.with(|count| count.replace(0))
}

#[cfg(test)]
pub(crate) fn note_full_artifact_record() {
    FULL_ARTIFACT_RECORDS.with(|count| count.set(count.get().saturating_add(1)));
}
