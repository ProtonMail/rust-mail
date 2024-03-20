use proton_crypto::crypto::VerificationError;
use proton_crypto_inbox::message::MessageDecryption;

mod common;
use common::{get_test_address_keys, get_test_public_address_keys};

const TEST_MESSAGE_BODY: &str = "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4Di5gBfuEszfESAQdAzv+eAfvm7tTd8GHvGn3Qsp2LhI2yjtKgSeg7uS69\nDV0w3DaikcJRSBvqQPWkkimzIdpyBIe4fzIaVERcUil0PTd+F8/zljGWTfNj\n29c030K90sBdATjoTBKarkG1Th7sllv1mC51vuxlvFateZmiLDNDeog6SdwM\n0YI9eKyT2+Wpyi9ehfw6HAwlMKDMY0ybFxhBCSpuWSZ9kIenGKJMym3MhkJM\nJu4J4F+PcZwO+katTJN4CnqyrGSOJYllECWqggZDdoF4nEm3G2LYI1W573Q6\no+fRqywqyPdHaqDiqviuL29RsqeG+Y+4TxQhXS2i4AfbhkBw1pv0fudTlNCu\nBSerK9SkpBKeDRxbfmmaRVPL0aFZjjwFYy0USg0JP0VEWClB0CCLiKhHvQsE\nUSy5VGT9ChsTRl2idtc2iUcfBUKiLT8JlAFfzFVW8WZgfpEEmUgSNS06/SQ/\ncaz1Mm9EF6xfkiBjxwDG7iEZSHIbzMCi\n=7AjW\n-----END PGP MESSAGE-----\n";
const TEST_EXPECTED_BODY: &str = r#"<div style="font-family: Arial, sans-serif; font-size: 14px;"><span>Test Attachment</span><br></div>
"#;

struct TestMessage(pub String);

impl MessageDecryption for TestMessage {
    fn message_is_mime(&self) -> bool {
        false
    }

    fn message_encrypted_body(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[test]
fn test_message_decrypt_and_verify() {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let decryption_keys = get_test_address_keys(&pgp_provider);
    let mut verification_keys = get_test_public_address_keys(&pgp_provider);
    let test_message = TestMessage(TEST_MESSAGE_BODY.into());
    let decrypted_message = test_message
        .decrypt(&pgp_provider, &decryption_keys)
        .unwrap();
    assert_eq!(decrypted_message.as_ref(), TEST_EXPECTED_BODY);
    let verification_result = decrypted_message.verify_signature(&pgp_provider, &verification_keys);
    assert!(verification_result.is_ok());
    verification_keys.remove(0);
    let verification_result_no_verifier =
        decrypted_message.verify_signature(&pgp_provider, &verification_keys);
    assert!(matches!(
        verification_result_no_verifier.unwrap_err(),
        VerificationError::NoVerifier(_)
    ));
}
