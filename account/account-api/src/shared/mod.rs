use derive_more::{Debug, Deref, Display, From};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub mod challenge;
pub mod crypto;

/// A secure string wrapper that automatically zeros its contents when dropped.
#[derive(Debug, Display, Deref, Clone, From, Zeroize, ZeroizeOnDrop)]
pub struct SecureString(
    #[debug(skip)]
    #[display(skip)]
    String,
);
