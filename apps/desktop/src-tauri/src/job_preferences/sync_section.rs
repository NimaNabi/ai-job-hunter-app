//! The optional self-hosted sync's view of this single-object section
//! (ADR-051): the whole object is one record.

crate::data_store::single_object_sync_section!(
    super::JobPreferencesStore,
    super::JobPreferencesStore::MIGRATIONS.len() as u32
);
