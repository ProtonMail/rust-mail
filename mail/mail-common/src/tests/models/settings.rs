use super::*;
use pretty_assertions::assert_eq;
use proton_mail_common::test_utils::db::new_test_connection;

#[tokio::test]
async fn test_mail_settings_store_read() {
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let mut settings = MailSettings {
        local_id: MailSettingsId,
        display_name: "foo".to_owned(),
        signature: "bar".to_owned(),
        theme: "goose".to_owned(),
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
        image_proxy: Default::default(),
        num_message_per_page: 0,
        draft_mime_type: Default::default(),
        receive_mime_type: Default::default(),
        show_mime_type: Default::default(),
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
        hide_embedded_images: Default::default(),
        hide_sender_images: Default::default(),
    };
    tether
        .tx::<_, _, StashError>(async |tx| settings.save(tx).await)
        .await
        .unwrap();
    let db_settings = MailSettings::get(&tether).await.unwrap().unwrap();
    assert_eq!(db_settings, settings);
    assert_eq!(db_settings.local_id, MailSettingsId);
}

#[tokio::test]
async fn test_mail_settings_updated() {
    let mut tether = new_test_connection().await.connection().await.unwrap();
    let mut settings = MailSettings {
        local_id: MailSettingsId,
        display_name: "foo".to_owned(),
        signature: "bar".to_owned(),
        theme: "goose".to_owned(),
        ..Default::default()
    };
    // Save once
    tether
        .tx::<_, _, StashError>(async |tx| settings.save(tx).await)
        .await
        .unwrap();

    let mut settings = MailSettings {
        local_id: MailSettingsId,
        display_name: "bar".into(), // We change foo into bar
        signature: "bar".to_owned(),
        theme: "goose".to_owned(),
        ..Default::default()
    };
    // Save second time
    tether
        .tx::<_, _, StashError>(async |tx| settings.save(tx).await)
        .await
        .unwrap();

    let db_settings = MailSettings::get(&tether).await.unwrap().unwrap();
    assert_eq!(db_settings, settings);
    assert_eq!(db_settings.local_id, MailSettingsId);
}
