/// An extension trait for [`muon::ProtonResponse`].
///
/// This trait is a general-purpose collection of utility methods for working with
/// responses from the Proton service.
///
/// It's currently (as of v0.59.0) unused but is left here for future use.
pub trait ProtonResponseExt {
    /// Inspects the body of the response, passing it as a byte slice to the given closure,
    /// returning the response unchanged.
    #[must_use]
    fn inspect_body(self, f: impl FnOnce(&[u8])) -> Self;
}

impl ProtonResponseExt for muon::ProtonResponse {
    fn inspect_body(self, f: impl FnOnce(&[u8])) -> Self {
        f(self.body());

        self
    }
}
