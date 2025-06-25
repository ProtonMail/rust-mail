#[derive(uniffi::Object)]
pub struct MuonClient {
    client: muon::Client,
}

impl MuonClient {
    #[must_use]
    pub fn new(client: &muon::Client) -> Self {
        Self {
            client: client.clone(),
        }
    }

    #[must_use]
    pub fn as_inner(&self) -> &muon::Client {
        &self.client
    }
}
