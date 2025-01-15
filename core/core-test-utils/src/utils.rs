use rand::{distributions::Uniform, Rng};
/// Generates a random string of the specified length, including alphanumeric and special characters.
///
/// # Parameters
/// - `length`: The length of the string to generate.
#[must_use]
pub fn random_string(length: usize) -> String {
    let charset: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                           abcdefghijklmnopqrstuvwxyz\
                           0123456789!@#$%^&*()_+-=[]{}|;:'\",.<>?/\\`~";

    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.sample(Uniform::new(0, charset.len()));
            charset[idx] as char
        })
        .collect()
}
