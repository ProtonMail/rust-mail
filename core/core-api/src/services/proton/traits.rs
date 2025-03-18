pub trait ProtonResponseExt {
    #[must_use]
    fn inspect_body(self, f: impl FnOnce(&[u8])) -> Self;
}

impl ProtonResponseExt for muon::ProtonResponse {
    fn inspect_body(self, f: impl FnOnce(&[u8])) -> Self {
        f(self.body());

        self
    }
}
