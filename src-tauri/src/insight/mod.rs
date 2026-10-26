pub(crate) mod detect;
mod map;
pub(crate) mod model;
mod otel;
mod poller;
mod slowlog;
mod stack;
pub(crate) mod store;

use sha2::{Digest, Sha256};

pub use poller::InsightPoller;
pub use store::{migrate, prune};

pub(crate) fn fingerprint_hash(text: &str) -> String {
    hex::encode(&Sha256::digest(text.as_bytes())[..8])
}
