//! Same names as `lib.rs`'s `orphan`, but a local and a field: neither is a
//! use of that function.

pub struct Holder {
    pub orphan: u32,
}

pub fn homonym_user(holder: &Holder) -> u32 {
    let orphan = 1;
    orphan + holder.orphan
}
