use crate::db::{new_test_connection, with_tx};
use proton_api_mail::domain::MailSettings;

#[test]
fn test_mail_settings_store_read() {
    let (_, mut conn, _) = new_test_connection();
    with_tx(&mut conn, |tx| {
        let settings = MailSettings {
            display_name: "foo".to_string(),
            signature: "bar".to_string(),
            theme: "goose".to_string(),
            auto_save_contacts: Default::default(),
            composer_mode: Default::default(),
            message_buttons: Default::default(),
            show_images: Default::default(),
            show_moved: Default::default(),
            auto_delete_spam_and_trash_days: None,
            almost_all_mail: Default::default(),
            next_message_on_move: None,
            view_mode: Default::default(),
            view_layout: Default::default(),
            swipe_left: Default::default(),
            swipe_right: Default::default(),
            shortcuts: Default::default(),
            pm_signature: Default::default(),
            pm_signature_referral_link: Default::default(),
            image_proxy: 0,
            num_message_per_page: 0,
            draft_mime_type: "text/html".to_string(),
            receive_mime_type: "text/html".to_string(),
            show_mime_type: "text/html".to_string(),
            enable_folder_color: Default::default(),
            inherit_parent_folder_color: Default::default(),
            submission_access: Default::default(),
            right_to_left: Default::default(),
            attach_public_key: Default::default(),
            sign: Default::default(),
            pgp_scheme: Default::default(),
            prompt_pin: Default::default(),
            sticky_labels: Default::default(),
            confirm_link: Default::default(),
            delay_send_seconds: 0,
            font_face: None,
            spam_action: None,
            block_sender_confirmation: None,
            mobile_settings: None,
            hide_remote_images: Default::default(),
            hide_sender_images: Default::default(),
        };
        tx.create_or_update_mail_settings(&settings).unwrap();
        let db_settings = tx.mail_settings().unwrap();
        assert_eq!(db_settings, settings);
    })
}
