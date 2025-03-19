use crate::{UniffiEnum, UniffiRecord};

/// Representation of User's Report of an issue.
#[derive(UniffiRecord)]
pub struct IssueReport {
    /// Name of the operating system app was run in.
    ///
    /// Provided by the client.
    ///
    /// # Example
    ///
    /// `iOS - iPhone`
    pub operating_system: String,

    /// Vesion of the operating system installed on the device.
    ///
    /// # Example
    ///
    /// `18.4`
    pub operating_system_version: String,

    /// Name of the client
    ///
    /// Provided by the client.
    ///
    /// # Example
    ///
    /// `iOS_Native`
    pub client: String,

    /// Version of the client application
    ///
    /// It is not verified but Semantic Versioning is encouraged.
    /// Provided by the client.
    ///
    /// # Example
    ///
    ///  `4.20.0`
    pub client_version: String,

    /// Type of client application
    ///
    /// Provided by the client.
    ///
    /// # Example
    ///
    /// `1` - Email
    pub client_type: ClientType,

    /// Common title for the client.
    ///
    /// Provided by the client.
    ///
    /// # Example
    ///
    /// `Proton Mail App bug report`
    pub title: String,

    /// Summary of the stumbled upon issue.
    ///
    /// The string has to be at least 10 characters long.
    /// Depicts incident, it is provided by the user.
    pub summary: String,

    /// The steps needed to reproduce the issue.
    ///
    /// Can be empty.
    /// Provided by the user.
    pub stepst_to_reproduce: String,

    /// User's expected behavior.
    ///
    /// Can be empty.
    /// Provided by the user.
    pub expected_result: String,

    /// What happened instead.
    ///
    /// Can be empty.
    /// Provided by the user.
    pub actual_result: String,

    /// Permission to attach the logs to the report.
    ///
    /// User gave permission to share the logs with bug report
    /// by selecting an option in the client app.
    pub logs: bool,
}

/// Representation of Client type
#[derive(UniffiEnum)]
pub enum ClientType {
    Email = 1,
}
