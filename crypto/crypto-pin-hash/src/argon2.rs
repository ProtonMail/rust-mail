use argon2::{
    Argon2,
    password_hash::{
        Error as Argon2Error, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
        rand_core::OsRng,
    },
};
use std::{fmt::Display, str::FromStr};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Debug, Error)]
pub enum Argon2HashingError {
    #[error("Failed to hash the password, details: `{0}`")]
    Hash(String),
}

impl From<Argon2Error> for Argon2HashingError {
    fn from(value: Argon2Error) -> Self {
        Self::Hash(value.to_string())
    }
}

/// Struct representing hash string.
///
#[derive(Clone, Debug, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct ProtonArgon2Hash(String);

impl Display for ProtonArgon2Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hash = &self.0;

        write!(f, "{hash}")
    }
}

impl FromStr for ProtonArgon2Hash {
    /// Parsing hash never throw an error
    /// It's here just for the type match
    type Err = Argon2HashingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

/// Hashes the password for authentication.
///
/// It's using Argon2 hashing algorithm.
///
pub fn hash<P: AsRef<[u8]>>(password: P) -> Result<ProtonArgon2Hash, Argon2HashingError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(password.as_ref(), &salt)?.to_string();

    Ok(ProtonArgon2Hash(password_hash))
}

/// Verifies a password against a stored Argon2 hash for authentication.
///
pub fn verify<P: AsRef<[u8]>>(
    password: P,
    hash: &ProtonArgon2Hash,
) -> Result<bool, Argon2HashingError> {
    Ok(Argon2::default()
        .verify_password(password.as_ref(), &PasswordHash::new(&hash.0)?)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "myfancyunbreakablepassword";

        let hash_1 = hash(password).unwrap();
        let hash_2 = hash(password).unwrap();

        assert_ne!(hash_1, hash_2);

        let hash_1_parsed = hash_1.to_string().parse().unwrap();
        let hash_2_parsed = hash_2.to_string().parse().unwrap();

        assert_eq!(hash_1_parsed, hash_1);
        assert_eq!(hash_2_parsed, hash_2);
        assert!(verify(password, &hash_1).unwrap());
        assert!(verify(password, &hash_2).unwrap());
        assert!(verify(password, &hash_1_parsed).unwrap());
        assert!(verify(password, &hash_2_parsed).unwrap());

        let incorrect_password = format!("{password}111mycatsatonthekeyboard111");
        assert!(!verify(incorrect_password, &hash_1).unwrap());
    }
}
