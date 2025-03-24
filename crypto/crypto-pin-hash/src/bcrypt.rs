use bcrypt::{BcryptError, DEFAULT_COST, hash as bchash, verify as bcverify};
use std::{fmt::Display, str::FromStr};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HashingError {
    #[error("Failed to hash the password, details: `{0}`")]
    Hash(#[from] BcryptError),
}

/// Struct representing Hash
///
#[derive(Clone, Debug, PartialEq)]
pub struct ProtonHash(String);

impl Display for ProtonHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hash = &self.0;

        write!(f, "{hash}")
    }
}

impl FromStr for ProtonHash {
    /// Parsing hash never throw an error
    /// Its here just for the type match
    type Err = HashingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

/// Hashes the password for authentication.
///
/// bcrypt already uses salt and BASE64 transformation
/// So we just wrap String in the newtype
///
pub fn hash<P: AsRef<[u8]>>(password: P) -> Result<ProtonHash, HashingError> {
    let hash = bchash(password, DEFAULT_COST)?;

    Ok(ProtonHash(hash))
}

/// Verifies a password against a stored hash for authentication.
///
pub fn verify<P: AsRef<[u8]>>(password: P, hash: &ProtonHash) -> Result<bool, HashingError> {
    Ok(bcverify(password, &hash.0)?)
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
