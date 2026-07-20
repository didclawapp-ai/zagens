//! @generated modules — see `scripts/generate-providers.py`.

#[path = "keyring_slots.rs"]
mod keyring_slots;

pub use keyring_slots::{KEYRING_SLOT_REGISTRY, canonicalize_keyring_slot};
