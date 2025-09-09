//! Rust bindings for the `go-crypto` library.

use std::num::TryFromIntError;
use std::string::FromUtf8Error;
use thiserror::Error;

mod gen;

/// An error that can occur when interacting with the `go-crypto` library.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum Error {
    /// An error converting an integer.
    Int(#[from] TryFromIntError),

    /// An error converting bytes to a string.
    Str(#[from] FromUtf8Error),

    /// An application-specific error.
    #[error("{0}")]
    App(&'static str),
}

/// Hashes the given password with the given salt.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn get_mbox_pass(pass: &str, salt: &[u8]) -> Result<Vec<u8>, Error> {
    let pass = to_go_slice(pass.as_bytes())?;
    let salt = to_go_slice(salt)?;

    let gen::getMboxPass_return {
        r0: pass,
        r1: result,
    } = unsafe { gen::getMboxPass(pass, salt) };

    if ok(result) {
        Ok(raw_to_vec(pass))
    } else {
        Err(Error::App("failed to generate mailbox password"))
    }
}

/// Creates a new PGP key locked with the given password.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn new_raw_pgp_key(name: &str, addr: &str, pass: &[u8]) -> Result<Vec<u8>, Error> {
    let name = to_go_slice(name.as_bytes())?;
    let addr = to_go_slice(addr.as_bytes())?;
    let pass = to_go_slice(pass)?;

    let gen::newRawPgpKey_return {
        r0: key,
        r1: result,
    } = unsafe { gen::newRawPgpKey(name, addr, pass) };

    if ok(result) {
        Ok(raw_to_vec(key))
    } else {
        Err(Error::App("failed to generate PGP key"))
    }
}

/// Unlocks the given raw PGP key using the given password.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn unlock_raw_pgp_key(raw_key: &[u8], pass: &[u8]) -> Result<Vec<u8>, Error> {
    let raw_key = to_go_slice(raw_key)?;
    let pass = to_go_slice(pass)?;

    let gen::unlockRawPgpKey_return {
        r0: key,
        r1: result,
    } = unsafe { gen::unlockRawPgpKey(raw_key, pass) };

    if ok(result) {
        Ok(raw_to_vec(key))
    } else {
        Err(Error::App("failed to unlock PGP key"))
    }
}

/// Unlocks the given armored PGP key using the given password.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn unlock_arm_pgp_key(arm_key: &str, pass: &[u8]) -> Result<Vec<u8>, Error> {
    let arm_key = to_go_slice(arm_key.as_bytes())?;
    let pass = to_go_slice(pass)?;

    let gen::unlockArmPgpKey_return {
        r0: key,
        r1: result,
    } = unsafe { gen::unlockArmPgpKey(arm_key, pass) };

    if ok(result) {
        Ok(raw_to_vec(key))
    } else {
        Err(Error::App("failed to unlock PGP key"))
    }
}

/// Armors the given raw PGP key.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn to_arm_pgp_key(raw_key: &[u8]) -> Result<String, Error> {
    let raw_key = to_go_slice(raw_key)?;

    let gen::toArmPgpKey_return {
        r0: key,
        r1: result,
    } = unsafe { gen::toArmPgpKey(raw_key) };

    if ok(result) {
        Ok(raw_to_string(key)?)
    } else {
        Err(Error::App("failed to armor PGP key"))
    }
}

/// Unarmors the given armored PGP key.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn to_raw_pgp_key(arm_key: &str) -> Result<Vec<u8>, Error> {
    let arm_key = to_go_slice(arm_key.as_bytes())?;

    let gen::toRawPgpKey_return {
        r0: key,
        r1: result,
    } = unsafe { gen::toRawPgpKey(arm_key) };

    if ok(result) {
        Ok(raw_to_vec(key))
    } else {
        Err(Error::App("failed to unarmor PGP key"))
    }
}

/// Decrypts the given raw message using the given keys.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn decrypt_raw_msg(raw_msg: &[u8], keys: &[&[u8]]) -> Result<Vec<u8>, Error> {
    let raw_msg = to_go_slice(raw_msg)?;

    let keys = (keys.iter().map(|key| to_go_slice(key))).collect::<Result<Vec<_>, _>>()?;
    let keys = to_go_slice(&keys)?;

    let gen::decryptRawMsg_return {
        r0: msg,
        r1: result,
    } = unsafe { gen::decryptRawMsg(raw_msg, keys) };

    if ok(result) {
        Ok(raw_to_vec(msg))
    } else {
        Err(Error::App("failed to decrypt message"))
    }
}

/// Decrypts the given split message using the given keys.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn decrypt_split_msg(pkts: &[u8], data: &[u8], keys: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let pkts = to_go_slice(pkts)?;
    let data = to_go_slice(data)?;

    let keys = (keys.iter().map(|key| to_go_slice(key))).collect::<Result<Vec<_>, _>>()?;
    let keys = to_go_slice(&keys)?;

    let gen::decryptSplitMsg_return {
        r0: msg,
        r1: result,
    } = unsafe { gen::decryptSplitMsg(pkts, data, keys) };

    if ok(result) {
        Ok(raw_to_vec(msg))
    } else {
        Err(Error::App("failed to decrypt message"))
    }
}

/// Decrypts the given armored message using the given keys.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn decrypt_arm_msg(arm_msg: &str, keys: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let arm_msg = to_go_slice(arm_msg.as_bytes())?;

    let keys = (keys.iter().map(|key| to_go_slice(key))).collect::<Result<Vec<_>, _>>()?;
    let keys = to_go_slice(&keys)?;

    let gen::decryptArmMsg_return {
        r0: msg,
        r1: result,
    } = unsafe { gen::decryptArmMsg(arm_msg, keys) };

    if ok(result) {
        Ok(raw_to_vec(msg))
    } else {
        Err(Error::App("failed to decrypt message"))
    }
}

/// Generates a new SRP salt.
#[must_use]
pub fn new_srp_salt() -> Vec<u8> {
    raw_to_vec(unsafe { gen::newSrpSalt() })
}

/// Generates a new SRP verifier from the given password and salt.
/// The SRP version, hashed password, and verifier are returned.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn new_srp_verifier(pass: &str, salt: &[u8]) -> Result<(u8, Vec<u8>, Vec<u8>), Error> {
    let pass = to_go_slice(pass.as_bytes())?;
    let salt = to_go_slice(salt)?;

    let gen::newSrpVerifier_return {
        r0: version,
        r1: hash,
        r2: verifier,
        r3: result,
    } = unsafe { gen::newSrpVerifier(pass, salt) };

    if ok(result) {
        Ok((version, raw_to_vec(hash), raw_to_vec(verifier)))
    } else {
        Err(Error::App("failed to generate verifier"))
    }
}

/// Begins a new SRP challenge with the given verifier.
///
/// The challenge ID, challenge data, and SRP modulus are returned.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created or the modulus cannot be
/// converted to a string.
pub fn new_srp_challenge(verifier: &[u8]) -> Result<(u64, Vec<u8>, String), Error> {
    let verifier = to_go_slice(verifier)?;

    let gen::newSrpChallenge_return {
        r0: id,
        r1: challenge,
        r2: modulus,
        r3: result,
    } = unsafe { gen::newSrpChallenge(verifier) };

    if ok(result) {
        Ok((id, raw_to_vec(challenge), raw_to_string(modulus)?))
    } else {
        Err(Error::App("failed to generate challenge"))
    }
}

/// Verifies the SRP proof for the given challenge ID.
///
/// If the proof is valid, the session key is returned.
///
/// # Errors
///
/// Returns an error if Go slices cannot be created.
pub fn verify_srp_proof(id: u64, ephem: &[u8], proof: &[u8]) -> Result<Vec<u8>, Error> {
    let ephem = to_go_slice(ephem)?;
    let proof = to_go_slice(proof)?;

    let gen::verifySrpProof_return {
        r0: key,
        r1: result,
    } = unsafe { gen::verifySrpProof(id, ephem, proof) };

    if ok(result) {
        Ok(raw_to_vec(key))
    } else {
        Err(Error::App("failed to verify proof"))
    }
}

fn ok(result: gen::GoInt8) -> bool {
    result == unsafe { gen::Ok() }
}

fn to_go_slice<T>(bytes: &[T]) -> Result<gen::GoSlice, TryFromIntError> {
    Ok(gen::GoSlice {
        data: bytes.as_ptr() as _,
        len: bytes.len().try_into()?,
        cap: bytes.len().try_into()?,
    })
}

fn raw_to_vec(raw: gen::Raw) -> Vec<u8> {
    unsafe {
        let vec = std::slice::from_raw_parts(raw.ptr, raw.len).to_vec();

        gen::freeRaw(raw);

        vec
    }
}

fn raw_to_string(raw: gen::Raw) -> Result<String, FromUtf8Error> {
    unsafe {
        let vec = std::slice::from_raw_parts(raw.ptr, raw.len).to_vec();

        gen::freeRaw(raw);

        String::from_utf8(vec)
    }
}
