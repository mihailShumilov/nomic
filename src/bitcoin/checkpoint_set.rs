use crate::bitcoin::adapter::Adapter;
use crate::bitcoin::signatory_set::Signatory;
use orga::collections::Entry;
use orga::collections::EntryMap;
use orga::prelude::{Call, Client, Query, State};
use std::ops::{Deref, DerefMut};

#[derive(Entry)]
pub struct Checkpoint {
    #[key]
    height: u64,
    checkpoint: EntryMap<Adapter<Signatory>>,
}

impl Checkpoint {
    pub fn new(height: u64, checkpoint: EntryMap<Adapter<Signatory>>) -> Self {
        Self { height, checkpoint }
    }

    pub fn checkpoint(&self) -> &EntryMap<Adapter<Signatory>> {
        &self.checkpoint
    }
}

#[derive(Call, Query, Client, State)]
pub struct CheckpointSet {
    inner: EntryMap<Checkpoint>,
}

impl Deref for CheckpointSet {
    type Target = EntryMap<Checkpoint>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for CheckpointSet {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
