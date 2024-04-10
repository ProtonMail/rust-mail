use crate::db::{CoreSqliteConnectionImpl, DBResult};
use proton_api_core::domain::{
    User, UserFlags, UserId, UserProductUsedSpace, UserSettings, UserSettings2FA,
    UserSettingsEmail, UserSettingsFlags, UserSettingsHighSecurity, UserSettingsPassword,
    UserSettingsPhone, UserSettingsReferral,
};
use proton_api_core::exports::crypto::domain::{KeyId, LockedKey, UserKeys};
use proton_sqlite3::rusqlite::{OptionalExtension, Row};
use proton_sqlite3::utils::{gen_variable_in_argument_list, mapped_rows_to_vec, RowIndexAllocator};
use proton_sqlite3::{bind_list_indexed, bind_list_indexed_recursive};

impl<'c> CoreSqliteConnectionImpl<'c> {
    pub fn create_or_update_user(&mut self, user: &User) -> DBResult<()> {
        let mut stmt = self.0.prepare(&format!(
            "INSERT OR REPLACE INTO users VALUES ({})",
            gen_variable_in_argument_list(24)
        ))?;
        bind_list_indexed!(
            &mut stmt,
            &user.id,
            &user.name,
            &user.display_name,
            &user.email,
            &user.currency,
            &user.credit,
            user.user_type,
            &user.create_time,
            &user.max_space,
            &user.max_upload,
            &user.used_space,
            user.role,
            user.private,
            user.to_migrate,
            user.mnemonic_status,
            user.subscribed,
            user.services,
            user.delinquent,
            user_flags_to_u32(&user.flags),
            user.product_used_space.calendar,
            user.product_used_space.contact,
            user.product_used_space.drive,
            user.product_used_space.mail,
            user.product_used_space.pass,
        );
        stmt.raw_execute()?;

        let mut key_stmt = self
            .0
            .prepare("INSERT OR REPLACE INTO user_keys VALUES (?,?,?,?,?,?,?,?)")?;
        for k in &user.keys.0 {
            key_stmt.execute((
                &user.id,
                k.id.as_ref(),
                k.version,
                &k.private_key,
                k.primary,
                k.active,
                &k.recovery_secret,
                &k.recovery_secret_signature,
            ))?;
        }

        Ok(())
    }

    pub fn get_user(&mut self, user_id: &UserId) -> DBResult<Option<User>> {
        let Some(mut user) = self
            .0
            .query_row(
                UserSelector::query_with_id(),
                [user_id],
                UserSelector::from_row,
            )
            .optional()?
        else {
            return Ok(None);
        };
        let mut key_stmt = self.0.prepare(UserKeySelector::query())?;
        let keys = mapped_rows_to_vec(key_stmt.query_map([user_id], UserKeySelector::from_row)?)?;
        user.keys = UserKeys(keys);
        Ok(Some(user))
    }

    pub fn update_user_used_space(&mut self, user_id: &UserId, used_space: i64) -> DBResult<()> {
        self.0.execute(
            "UPDATE users SET used_space=? WHERE id=?",
            (used_space, user_id),
        )?;
        Ok(())
    }

    pub fn update_user_product_used_space(
        &mut self,
        user_id: &UserId,
        used_space: &UserProductUsedSpace,
    ) -> DBResult<()> {
        self.0.execute(
            "UPDATE users SET pus_calendar=?, pus_contact=?, pus_drive=?, \
    pus_mail=?, pus_pass=? WHERE id=?",
            (
                used_space.calendar,
                used_space.contact,
                used_space.drive,
                used_space.mail,
                used_space.pass,
                user_id,
            ),
        )?;
        Ok(())
    }

    pub fn create_or_update_user_settings(
        &mut self,
        user_id: &UserId,
        settings: &UserSettings,
    ) -> DBResult<()> {
        let mut stmt = self.0.prepare(&format!(
            "INSERT OR REPLACE INTO user_settings VALUES ({})",
            gen_variable_in_argument_list(35)
        ))?;
        let (referral_link, referral_eligible) = if let Some(settings) = &settings.referral {
            (Some(&settings.link), Some(settings.eligible))
        } else {
            (None, None)
        };
        bind_list_indexed!(
            &mut stmt,
            user_id,
            &settings.email.value,
            settings.email.status,
            settings.email.notify,
            settings.email.reset,
            settings.password.mode,
            settings.password.expiration_time,
            &settings.phone.value,
            settings.phone.status,
            settings.phone.notify,
            settings.phone.reset,
            settings.two_factor_auth.enabled,
            settings.two_factor_auth.allowed,
            settings.two_factor_auth.expiration_time,
            settings.news,
            &settings.locale,
            settings.log_auth,
            &settings.invoice_text,
            settings.density,
            settings.week_start,
            settings.date_format,
            settings.time_format,
            settings.welcome,
            settings.early_access,
            settings.flags.welcomed,
            settings.flags.in_app_promos_hidden,
            &referral_link,
            &referral_eligible,
            settings.device_recovery,
            settings.telemetry,
            settings.crash_reports,
            settings.hide_side_panel,
            settings.high_security.eligible,
            settings.high_security.value,
            settings.session_account_recovery,
        );
        stmt.raw_execute()?;
        Ok(())
    }

    pub fn get_user_settings(&self, user_id: &UserId) -> DBResult<Option<UserSettings>> {
        self.0
            .query_row(
                UserSettingsSelector::query(),
                [user_id],
                UserSettingsSelector::from_row,
            )
            .optional()
    }
}

struct UserSelector {}

impl UserSelector {
    fn query_with_id() -> &'static str {
        "SELECT * FROM users WHERE id=?"
    }

    fn from_row(r: &Row) -> DBResult<User> {
        Ok(User {
            id: r.get(0)?,
            name: r.get(1)?,
            display_name: r.get(2)?,
            email: r.get(3)?,
            currency: r.get(4)?,
            credit: r.get(5)?,
            user_type: r.get(6)?,
            create_time: r.get(7)?,
            max_space: r.get(8)?,
            max_upload: r.get(9)?,
            used_space: r.get(10)?,
            role: r.get(11)?,
            private: r.get(12)?,
            to_migrate: r.get(13)?,
            mnemonic_status: r.get(14)?,
            subscribed: r.get(15)?,
            services: r.get(16)?,
            delinquent: r.get(17)?,
            flags: user_flags_from_u32(r.get(18)?),
            product_used_space: UserProductUsedSpace {
                calendar: r.get(19)?,
                contact: r.get(20)?,
                drive: r.get(21)?,
                mail: r.get(22)?,
                pass: r.get(23)?,
            },
            keys: UserKeys(Vec::new()),
        })
    }
}

struct UserKeySelector {}

impl UserKeySelector {
    fn query() -> &'static str {
        "SELECT key_id, version, private_key, `primary`, active, recovery_secret, \
 recovery_secret_signature FROM user_keys WHERE user_id=?"
    }

    fn from_row(r: &Row) -> DBResult<LockedKey> {
        Ok(LockedKey {
            id: KeyId::from(r.get::<usize, String>(0)?),
            version: r.get(1)?,
            private_key: r.get(2)?,
            token: None,
            signature: None,
            activation: None,
            primary: r.get(3)?,
            active: r.get(4)?,
            flags: None,
            recovery_secret: r.get(5)?,
            recovery_secret_signature: r.get(6)?,
            address_forwarding_id: None,
        })
    }
}

struct UserSettingsSelector {}

impl UserSettingsSelector {
    fn query() -> &'static str {
        "SELECT * FROM user_settings WHERE id=?"
    }

    fn from_row(r: &Row) -> DBResult<UserSettings> {
        //TODO: compile time index generation?
        let mut alloc = RowIndexAllocator::new();

        fn referral_from_row(
            r: &Row,
            alloc: &mut RowIndexAllocator,
        ) -> DBResult<Option<UserSettingsReferral>> {
            let link: Option<String> = r.get(alloc.fetch_and_add())?;
            let eligible: Option<bool> = r.get(alloc.fetch_and_add())?;
            Ok(if let (Some(link), Some(eligible)) = (link, eligible) {
                Some(UserSettingsReferral { link, eligible })
            } else {
                None
            })
        }

        // advance once to skip ove user_id;
        alloc.fetch_and_add();
        Ok(UserSettings {
            email: UserSettingsEmail {
                value: r.get(alloc.fetch_and_add())?,
                status: r.get(alloc.fetch_and_add())?,
                notify: r.get(alloc.fetch_and_add())?,
                reset: r.get(alloc.fetch_and_add())?,
            },
            password: UserSettingsPassword {
                mode: r.get(alloc.fetch_and_add())?,
                expiration_time: r.get(alloc.fetch_and_add())?,
            },
            phone: UserSettingsPhone {
                value: r.get(alloc.fetch_and_add())?,
                status: r.get(alloc.fetch_and_add())?,
                notify: r.get(alloc.fetch_and_add())?,
                reset: r.get(alloc.fetch_and_add())?,
            },
            two_factor_auth: UserSettings2FA {
                enabled: r.get(alloc.fetch_and_add())?,
                allowed: r.get(alloc.fetch_and_add())?,
                expiration_time: r.get(alloc.fetch_and_add())?,
                registered_keys: Vec::new(),
            },
            news: r.get(alloc.fetch_and_add())?,
            locale: r.get(alloc.fetch_and_add())?,
            log_auth: r.get(alloc.fetch_and_add())?,
            invoice_text: r.get(alloc.fetch_and_add())?,
            density: r.get(alloc.fetch_and_add())?,
            week_start: r.get(alloc.fetch_and_add())?,
            date_format: r.get(alloc.fetch_and_add())?,
            time_format: r.get(alloc.fetch_and_add())?,
            welcome: r.get(alloc.fetch_and_add())?,
            early_access: r.get(alloc.fetch_and_add())?,
            flags: UserSettingsFlags {
                welcomed: r.get(alloc.fetch_and_add())?,
                in_app_promos_hidden: r.get(alloc.fetch_and_add())?,
            },
            referral: referral_from_row(r, &mut alloc)?,
            device_recovery: r.get(alloc.fetch_and_add())?,
            telemetry: r.get(alloc.fetch_and_add())?,
            crash_reports: r.get(alloc.fetch_and_add())?,
            hide_side_panel: r.get(alloc.fetch_and_add())?,
            high_security: UserSettingsHighSecurity {
                eligible: r.get(alloc.fetch_and_add())?,
                value: r.get(alloc.fetch_and_add())?,
            },
            session_account_recovery: r.get(alloc.fetch_and_add())?,
        })
    }
}

fn user_flags_to_u32(flags: &UserFlags) -> u32 {
    let mut v = 0_u32;
    v |= flags.protected as u32;
    v |= (flags.onboard_checklist_storage_granted as u32) << 1;
    v |= (flags.has_temporary_password as u32) << 2;
    v |= (flags.test_account as u32) << 3;
    v |= (flags.no_login as u32) << 4;
    v |= (flags.recovery_attempt as u32) << 5;
    v |= (flags.sso as u32) << 6;
    v |= (flags.no_proton_address as u32) << 7;
    v
}

fn user_flags_from_u32(v: u32) -> UserFlags {
    UserFlags {
        protected: (v & 0x01) != 0,
        onboard_checklist_storage_granted: ((v >> 1) & 0x01) != 0,
        has_temporary_password: ((v >> 2) & 0x01) != 0,
        test_account: ((v >> 3) & 0x01) != 0,
        no_login: ((v >> 4) & 0x01) != 0,
        recovery_attempt: ((v >> 5) & 0x01) != 0,
        sso: ((v >> 6) & 0x01) != 0,
        no_proton_address: ((v >> 7) & 0x01) != 0,
    }
}
