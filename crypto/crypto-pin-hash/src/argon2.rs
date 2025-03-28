use argon2::{
    Argon2,
    password_hash::{
        Error as Argon2Error, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
        rand_core::OsRng,
    },
};
use std::str::FromStr;
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

impl AsRef<str> for ProtonArgon2Hash {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl FromStr for ProtonArgon2Hash {
    type Err = core::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl ProtonArgon2Hash {
    /// Hashes the password for authentication.
    ///
    /// It's using Argon2 hashing algorithm.
    ///
    pub fn hash<P: AsRef<[u8]>>(password: P) -> Result<Self, Argon2HashingError> {
        let salt = SaltString::generate(&mut OsRng);
        // Default parameters: memory = 19 MiB, iterations = 2, parallelism = 1
        let argon2 = Argon2::default();
        let password_hash = argon2.hash_password(password.as_ref(), &salt)?.to_string();

        Ok(Self(password_hash))
    }

    /// Verifies a password against a stored Argon2 hash for authentication.
    ///
    pub fn verify<P: AsRef<[u8]>>(&self, password: P) -> Result<bool, Argon2HashingError> {
        Ok(Argon2::default()
            .verify_password(password.as_ref(), &PasswordHash::new(&self.0)?)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "myfancyunbreakablepassword";

        let hash_1 = ProtonArgon2Hash::hash(password).unwrap();
        let hash_2 = ProtonArgon2Hash::hash(password).unwrap();

        assert_ne!(hash_1, hash_2);

        let hash_1_parsed = hash_1.as_ref().parse::<ProtonArgon2Hash>().unwrap();
        let hash_2_parsed = hash_2.as_ref().parse::<ProtonArgon2Hash>().unwrap();

        assert_eq!(hash_1_parsed, hash_1);
        assert_eq!(hash_2_parsed, hash_2);
        assert!(hash_1.verify(password).unwrap());
        assert!(hash_2.verify(password).unwrap());
        assert!(hash_1_parsed.verify(password).unwrap());
        assert!(hash_2_parsed.verify(password).unwrap());

        let incorrect_password = format!("{password}111mycatsatonthekeyboard111");
        assert!(!hash_1.verify(incorrect_password).unwrap());
    }
}
