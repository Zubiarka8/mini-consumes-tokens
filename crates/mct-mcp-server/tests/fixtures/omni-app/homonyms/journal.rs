//! `post` is defined twice (`Journal::post` here, `Draft::post` in
//! `draft.rs`); each call names its type, so neither is the other's.

pub struct Journal;

impl Journal {
    pub fn post(&self) {}
}

pub fn publish(journal: &Journal) {
    Journal::post(journal);
}

pub fn discard(draft: &Draft) {
    Draft::post(draft);
}
