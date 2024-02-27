use std::fs::{self, File};
use std::path::PathBuf;

use proton_crypto_inbox::attachment::{Attachment, AttachmentCrypto};
use proton_crypto_inbox::proton_crypto::crypto::{DataEncoding, PGPProviderSync};

#[test]
fn test_attachment_decrypt() {
    let root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-example");
    let attachment_metadata_json =
        File::open(root_path.clone().join("attachment_metadata.json")).unwrap();
    let attachment_metadata: Attachment =
        serde_json::from_reader(attachment_metadata_json).unwrap();

    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let private_key =
        fs::read_to_string(root_path.clone().join("receiver_decryption_key.asc")).unwrap();
    let imported_key = pgp_provider
        .private_key_import(private_key, "password", DataEncoding::Armor)
        .unwrap();
    let decryption_keys = vec![imported_key];
    let verification_keys = Vec::new();
    let enc_data: Vec<u8> = fs::read(root_path.clone().join("attachment")).unwrap();
    let decrypted_attachment = attachment_metadata
        .decrypt_attachment(
            &pgp_provider,
            &decryption_keys,
            &verification_keys,
            enc_data,
        )
        .unwrap();
    let expected_data: Vec<u8> = fs::read(root_path.clone().join("out.png")).unwrap();
    assert_eq!(decrypted_attachment.as_ref(), &expected_data)
}

#[test]
fn test_attachment_decrypt_stream() {
    let pgp_provider = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-example");
    let attachment_metadata_json =
        File::open(root_path.clone().join("attachment_metadata.json")).unwrap();
    let attachment_metadata: Attachment =
        serde_json::from_reader(attachment_metadata_json).unwrap();
    let private_key =
        fs::read_to_string(root_path.clone().join("receiver_decryption_key.asc")).unwrap();
    let imported_key = pgp_provider
        .private_key_import(private_key, "password", DataEncoding::Armor)
        .unwrap();
    let decryption_keys = vec![imported_key];
    let verification_keys = Vec::new();
    let enc_data = File::open(root_path.clone().join("attachment")).unwrap();
    let mut output_buffer = Vec::new();
    let _verification_result = attachment_metadata
        .decrypt_attachment_from_reader(
            &pgp_provider,
            &decryption_keys,
            &verification_keys,
            enc_data,
            &mut output_buffer,
        )
        .unwrap();
    let expected_data: Vec<u8> = fs::read(root_path.clone().join("out.png")).unwrap();
    assert_eq!(&output_buffer, &expected_data)
}
