/// Represents the password change flow state where we're waiting for 2FA authentication.
#[derive(Clone, Copy)]
pub struct WantTfa {
    pub change_master_password: bool,
}

impl WantTfa {
    #[must_use]
    pub fn for_changing_master_password() -> Self {
        Self {
            change_master_password: true,
        }
    }

    #[must_use]
    pub fn for_changing_password() -> Self {
        Self {
            change_master_password: false,
        }
    }
}
