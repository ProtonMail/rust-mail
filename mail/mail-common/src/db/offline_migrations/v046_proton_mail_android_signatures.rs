use proton_sqlite3::Migration;
use stash::{
    UserDb, params,
    stash::{Bond, StashError},
};

pub struct AndroidSignaturesMigration;

impl AndroidSignaturesMigration {
    fn list() -> Vec<(String, String)> {
        let signatures = vec![
            "Skickat från Proton Mail Android",
            "Enviado desde Proton Mail para Android",
            "Trimis cu Proton Mail pentru Android",
            "Отправлено из мобильного приложения Proton Mail Android",
            "從 Proton Mail Android版送出",
            "Inviato da Proton Mail Android",
            "Enviat des de Proton Mail Android",
            "Odesláno z Proton Mail pro Android",
            "通过 Proton Mail 移动端发送",
            "Dikirim dari Proton Mail Android",
            "Proton Mail Android から送信",
            "Στάλθηκε από το Proton Mail Android",
            "Sendt fra Proton Mail Android",
            "Enviado via Proton Mail para Android",
            "Адпраўлена з ProtonMail для Android",
            "Sent from Proton Mail Android",
            "Wysłano z aplikacji Proton Mail",
            "Poslano iz Proton Mail Android",
            "Odoslané z Proton Mail pre Android",
            "Enviado do Proton Mail para Android",
            "Proton Mail Android ile gönderildi",
            "Enviado desde Proton Mail para Android",
            "Sendt fra Proton Mail Android",
            "Lähetetty Proton Mailin Android-sovelluksesta",
            "Sent from Proton Mail Android",
            "Envoyé depuis Proton Mail pour Android",
            "Poslano iz Proton Maila za Android",
            "Proton Mail Android alkalmazásból küldve",
            "Verzonden met Proton Mail Android",
            "Proton Mail Android से भेजा गया",
            "გამოგზავნილია Proton Mail Android-დან",
            "Gesendet von Proton Mail für Android",
            "Android용 Proton Mail에서 발송됨",
            "Надіслано з Proton Mail Android",
        ];

        signatures
            .into_iter()
            .map(|old| {
                let replace_from = if old.contains("Proton Mailin") {
                    "Proton Mailin"
                } else if old.contains("Proton Maila") {
                    "Proton Maila"
                } else if old.contains("Proton Mail에서") {
                    "Proton Mail에서"
                } else if old.contains("ProtonMail") {
                    "ProtonMail"
                } else {
                    "Proton Mail"
                };

                let replace_to = format!(
                    "<a target=\"_blank\" href=\"https://proton.me/mail/home\">{replace_from}</a>"
                );

                let new = old.replace(replace_from, &replace_to);

                (old.into(), new)
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl Migration<UserDb> for AndroidSignaturesMigration {
    fn name(&self) -> &str {
        "v046_proton_mail_android_signatures"
    }

    async fn migrate(&self, tx: &Bond<'_>) -> Result<(), StashError> {
        for (old, new) in Self::list() {
            tx.execute(
                "UPDATE custom_settings SET mobile_signature = ? WHERE trim(mobile_signature) = ?",
                params![new, old],
            )
            .await?;
        }

        Ok(())
    }
}
