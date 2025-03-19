use crate::{UniffiEnum, UniffiRecord};

#[derive(UniffiRecord)]
pub struct IssueReport {
    pub operating_system: String,
    pub operating_system_version: String,
    pub client: String,
    pub client_version: String,
    pub client_type: ClientType,
    pub title: String,
    pub summary: String,
    pub stepst_to_reproduce: String,
    pub expected_result: String,
    pub actual_result: String,
    pub logs: bool,
}

#[derive(UniffiEnum)]
pub enum ClientType {
    Email = 1,
}
