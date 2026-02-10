//! ## App
//!
//! This module defines the types representing an application in Muon. An
//! application can be represented in term of name, version, platform,
//! user-agent, etc.  
//!
//! An application is an essential element in Muon. Each client are created for
//! a specific application and the application's information are transmitted to
//! the Proton API.
//!
//! ### Example
//!
//! ```
//! # fn display_update_modal() {}
//! # use anyhow::*;
//! use muon::App;
//! use muon::app::{Platform, SemVer};
//! # fn main() -> anyhow::Result<()> {
//! // create an app, with a custom user-agent
//! let app = App::new("web-drive@1.0.0")?.with_user_agent("Mozilla/5.0");
//! // get the app version
//! let app_version = app.app_version();
//! // retrieve the version only
//! let Some(ver) = app_version.version() else {
//!     anyhow::bail!("un-versioned app");
//! };
//! // retrieve the app. name
//! let Some(app_name) = app_version.name() else {
//!     anyhow::bail!("un-named app");
//! };
//! // retrieve the platform
//! let platform = app_name.platform();
//! // if the platform is android and the version is too old, display a modal
//! if (ver == &"2.0.0.0".parse::<SemVer>()?) && *platform == Platform::Android {
//!     display_update_modal();
//! }
//! # Ok(())
//! # }
//! ```

use crate::util::IntoIterExt;
use derive_more::{AsRef, Deref, Display, FromStr};
use semver::Error as SemVerError;
use std::borrow::Borrow;
use std::fmt::{Formatter, Result as FmtResult};
use std::num::ParseIntError;
use thiserror::Error;

/// Represents an app using the `muon` client.
#[must_use]
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
#[display("{version} ({agent})")]
pub struct App {
    version: AppVersion,
    agent: UserAgent,
}

if_unsealed! {
    impl Default for App {
        fn default() -> Self {
            Self {
                version: AppVersion::Other,
                agent: UserAgent::default(),
            }
        }
    }
}

impl App {
    /// Create a new app from the given app version string.
    /// The user agent is set to the default value of `None`.
    ///
    /// # Example
    ///
    /// ```
    /// use muon::App;
    ///
    /// assert!(App::new("web-drive@1.0.0").is_ok());
    /// assert!(App::new("foo-bar@a.b.c").is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the app version is invalid.
    pub fn new(version: impl AsRef<str>) -> Result<Self, ParseAppVersionErr> {
        let version = version.as_ref().parse()?;
        let agent = UserAgent::default();

        Ok(Self { version, agent })
    }

    /// Set the user agent on this app.
    ///
    /// # Example
    ///
    /// ```
    /// use muon::App;
    ///
    /// let app = App::new("web-drive@1.0.0").unwrap();
    /// assert_eq!(format!("{app}"), "web-drive@1.0.0 (None)");
    ///
    /// let app = app.with_user_agent("Mozilla/5.0");
    /// assert_eq!(format!("{app}"), "web-drive@1.0.0 (Mozilla/5.0)");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the app version or user agent is invalid.
    pub fn with_user_agent(self, agent: impl AsRef<str>) -> Self {
        let agent = UserAgent(agent.as_ref().to_owned());

        Self { agent, ..self }
    }

    /// Get the app version.
    #[must_use]
    pub fn app_version(&self) -> &AppVersion {
        &self.version
    }

    /// Get the user agent.
    #[must_use]
    pub fn user_agent(&self) -> &UserAgent {
        &self.agent
    }
}

impl AsRef<AppVersion> for &App {
    fn as_ref(&self) -> &AppVersion {
        &self.version
    }
}

impl AsRef<UserAgent> for &App {
    fn as_ref(&self) -> &UserAgent {
        &self.agent
    }
}

/// An app version parse error.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum ParseAppVersionErr {
    /// The name is invalid.
    Name(#[from] ParseAppNameErr),

    /// The version is invalid.
    Version(#[from] ParseSemVerErr),

    /// The app version is missing a component.
    #[error("incomplete app version: {0}")]
    Incomplete(String),
}

/// Represents the version of an app.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
pub enum AppVersion {
    /// A named app.
    #[display("{name}@{version}")]
    Named {
        /// The name of the app, with its platform, product, and section.
        name: AppName,

        /// The semantic version of the app.
        version: SemVer,
    },

    /// An unnamed app.
    Other,
}

impl AppVersion {
    /// Get the name of this app, if not `Other`.
    #[must_use]
    pub fn name(&self) -> Option<&AppName> {
        if let Self::Named { name, .. } = self {
            Some(name)
        } else {
            None
        }
    }

    /// Get the version of this app, if not `Other`.
    #[must_use]
    pub fn version(&self) -> Option<&SemVer> {
        if let Self::Named { version, .. } = self {
            Some(version)
        } else {
            None
        }
    }
}

impl FromStr for AppVersion {
    type Err = ParseAppVersionErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("other") {
            return Ok(Self::Other);
        }

        let Some((name, version)) = s.split_once('@') else {
            return Err(ParseAppVersionErr::Incomplete(s.to_owned()));
        };

        Ok(Self::Named {
            name: name.parse()?,
            version: version.parse()?,
        })
    }
}

/// A name parse error.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum ParseAppNameErr {
    /// A platform parse error.
    Platform(#[from] ParsePlatformErr),

    /// A product parse error.
    Product(#[from] ParseProductErr),

    /// The name is invalid.
    #[error("invalid name: {0}")]
    Invalid(String),
}

/// The name of an app.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppName {
    /// The platform of the app.
    platform: Platform,

    /// The product of the app.
    product: Product,

    /// The section of the app, if any.
    section: Option<Section>,
}

impl AppName {
    fn new(platform: &str, product: &str, section: Option<&str>) -> Result<Self, ParseAppNameErr> {
        Ok(Self {
            platform: platform.parse()?,
            product: product.parse()?,
            section: section.map(|s| Section(s.to_owned())),
        })
    }

    /// Get the platform of this app.
    #[must_use]
    pub fn platform(&self) -> &Platform {
        &self.platform
    }

    /// Get the product of this app.
    #[must_use]
    pub fn product(&self) -> &Product {
        &self.product
    }

    /// Get the section of this app.
    #[must_use]
    pub fn section(&self) -> Option<&Section> {
        self.section.as_ref()
    }
}

impl Display for AppName {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        let AppName {
            platform,
            product,
            section,
        } = self;

        if let Some(section) = section {
            write!(f, "{platform}-{product}-{section}")
        } else {
            write!(f, "{platform}-{product}")
        }
    }
}

impl FromStr for AppName {
    type Err = ParseAppNameErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split('-').into_vec().as_slice() {
            // --- Two-part ---
            [plat, prod] => Ok(Self::new(plat, prod, None)?),

            // --- Three-part ---
            [plat, prod, sect] => Ok(Self::new(plat, prod, Some(sect))?),

            // --- Invalid ---
            _ => Err(ParseAppNameErr::Invalid(s.to_owned())),
        }
    }
}

/// A semver parse error.
#[derive(Debug, Error)]
#[error(transparent)]
pub enum ParseSemVerErr {
    /// A semver parse error.
    #[error(transparent)]
    SemVer(#[from] SemVerError),

    /// An integer parse error.
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),

    /// The semver is missing a component.
    #[error("incomplete semver: {0}")]
    Incomplete(String),
}

/// This type represents a semantic version with an optional build version.
///
/// The proton semver is a semver with an optional build version. Standard
/// semver requires `MAJOR.MINOR.PATCH` components, but we allow a fourth
/// component for the build version, which is optional.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemVer {
    inner: semver::Version,
    build: Option<u64>,
}

impl SemVer {
    /// Create a new semver from its string components.
    fn new(
        major: &str,
        minor: &str,
        patch: &str,
        build: Option<&str>,
        pre: Option<&str>,
        meta: Option<&str>,
    ) -> Result<Self, ParseSemVerErr> {
        let mut inner = format!("{major}.{minor}.{patch}");

        if let Some(pre) = pre {
            inner.push_str(&format!("-{pre}"));
        }

        if let Some(meta) = meta {
            inner.push_str(&format!("+{meta}"));
        }

        Ok(Self {
            inner: inner.parse()?,
            build: build.map(str::parse).transpose()?,
        })
    }
}

impl Display for SemVer {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        let semver::Version {
            major,
            minor,
            patch,
            ..
        } = &self.inner;

        write!(f, "{major}.{minor}.{patch}")?;

        if let Some(build) = &self.build {
            write!(f, ".{build}")?;
        }

        if !self.inner.pre.is_empty() {
            write!(f, "-{}", self.inner.pre)?;
        }

        if !self.inner.build.is_empty() {
            write!(f, "+{}", self.inner.build)?;
        }

        Ok(())
    }
}

impl FromStr for SemVer {
    type Err = ParseSemVerErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ver, meta) = match s.split_once('+') {
            Some((ver, meta)) => (ver, Some(meta)),
            None => (s, None),
        };

        let (ver, pre) = match ver.split_once('-') {
            Some((v, build)) => (v, Some(build)),
            None => (ver, None),
        };

        match ver.split('.').into_vec().as_slice() {
            [maj, min, pat] => SemVer::new(maj, min, pat, None, pre, meta),
            [maj, min, pat, opt] => SemVer::new(maj, min, pat, Some(opt), pre, meta),
            _ => Err(ParseSemVerErr::Incomplete(ver.to_owned())),
        }
    }
}

/// A platform parse error.
#[derive(Debug, Error)]
#[error("invalid platform: {0}")]
pub struct ParsePlatformErr(String);

/// The platform of an app.
#[non_exhaustive]
#[allow(non_camel_case_types)]
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
pub enum Platform {
    /// The android platform.
    #[display("android")]
    Android,

    /// The android TV platform.
    #[display("androidtv")]
    AndroidTV,

    /// The apple TV platform.
    #[display("appletv")]
    AppleTV,

    /// The browser platform.
    #[display("browser")]
    Browser,

    /// The desktop platform.
    #[display("desktop")]
    Desktop,

    /// The iOS platform.
    #[display("ios")]
    iOS,

    /// The linux platform.
    #[display("linux")]
    Linux,

    /// The macOS platform.
    #[display("macos")]
    macOS,

    /// The web platform.
    #[display("web")]
    Web,

    /// The windows platform.
    #[display("windows")]
    Windows,

    /// A custom platform
    #[cfg(feature = "other-platform")]
    #[display("{_0}")]
    Other(String),
}

impl FromStr for Platform {
    type Err = ParsePlatformErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Platform::*;

        match s.to_lowercase().as_str() {
            "android" => Ok(Android),
            "androidtv" => Ok(AndroidTV),
            "appletv" => Ok(AppleTV),
            "browser" => Ok(Browser),
            "desktop" => Ok(Desktop),
            "ios" => Ok(iOS),
            "linux" => Ok(Linux),
            "macos" => Ok(macOS),
            "web" => Ok(Web),
            "windows" => Ok(Windows),
            #[cfg(feature = "other-platform")]
            s => Ok(Other(s.to_owned())),
            #[cfg(not(feature = "other-platform"))]
            _ => Err(ParsePlatformErr(s.to_owned())),
        }
    }
}

/// A product parse error.
#[derive(Debug, Error)]
#[error("invalid product: {0}")]
pub struct ParseProductErr(String);

/// The product using the client.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display)]
pub enum Product {
    /// The account product.
    #[display("account")]
    Account,

    /// The bridge product.
    #[display("bridge")]
    Bridge,

    /// The calendar product.
    #[display("calendar")]
    Calendar,

    /// The contacts product.
    #[display("contacts")]
    Contacts,

    /// The docs product.
    #[display("docs")]
    Docs,

    /// The drive product.
    #[display("drive")]
    Drive,

    /// The mail product.
    #[display("mail")]
    Mail,

    /// The pass product.
    #[display("pass")]
    Pass,

    /// The vpn product.
    #[display("vpn")]
    Vpn,

    /// The wallet product.
    #[display("wallet")]
    Wallet,

    /// The authenticator product.
    #[display("authenticator")]
    Authenticator,

    /// Custom product
    #[cfg(feature = "other-product")]
    #[display("{_0}")]
    Other(String),
}

impl FromStr for Product {
    type Err = ParseProductErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use Product::*;

        match s.to_lowercase().as_str() {
            "account" => Ok(Account),
            "bridge" => Ok(Bridge),
            "calendar" => Ok(Calendar),
            "contacts" => Ok(Contacts),
            "docs" => Ok(Docs),
            "drive" => Ok(Drive),
            "mail" => Ok(Mail),
            "pass" => Ok(Pass),
            "vpn" => Ok(Vpn),
            "wallet" => Ok(Wallet),
            "authenticator" => Ok(Authenticator),
            #[cfg(feature = "other-product")]
            s => Ok(Other(s.to_owned())),
            #[cfg(not(feature = "other-product"))]
            _ => Err(ParseProductErr(s.to_owned())),
        }
    }
}

/// Represents a section of an app.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
pub struct Section(String);

impl AsRef<str> for Section {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for Section {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<str> for Section {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Represents a user agent.
#[derive(Debug, Display, Clone, PartialEq, Eq, Hash)]
pub struct UserAgent(String);

impl Default for UserAgent {
    fn default() -> Self {
        Self("None".to_owned())
    }
}

impl AsRef<str> for UserAgent {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for UserAgent {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Borrow<str> for UserAgent {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    /// Like `vec!` but for maps.
    #[doc(hidden)]
    macro_rules! map {
        ($($k:expr => $v:expr),* $(,)?) => {
            vec![$(($k, $v)),*].into_iter().collect::<std::collections::HashMap<_, _>>()
        };
    }

    #[test]
    fn test_app_valid() -> Result<()> {
        for ((version, agent), want) in map! {
            ("web-drive@1.0.0", None) => "web-drive@1.0.0 (None)",
            ("web-drive@1.0.0", Some("Mozilla/5.0")) => "web-drive@1.0.0 (Mozilla/5.0)",
            ("web-drive@1.0.0.0", None) => "web-drive@1.0.0.0 (None)",
            ("web-drive@5.0.999.999", Some("Foo")) => "web-drive@5.0.999.999 (Foo)",
        } {
            let mut app = App::new(version)?;

            if let Some(agent) = agent {
                app = app.with_user_agent(agent);
            }

            assert_eq!(app.to_string(), want);
        }

        Ok(())
    }

    #[test]
    fn test_app_invalid() {
        // Incomplete app version.
        assert!(matches!(
            App::new("web-drive"),
            Err(ParseAppVersionErr::Incomplete(_))
        ));

        // Invalid semver.
        assert!(matches!(
            App::new("web-drive@1.0"),
            Err(ParseAppVersionErr::Version(_))
        ));

        // Invalid product.
        #[cfg(not(feature = "other-product"))]
        assert!(matches!(
            App::new("web-foo@1.0.0"),
            Err(ParseAppVersionErr::Name(ParseAppNameErr::Product(_)))
        ));

        // Invalid platform.
        #[cfg(not(feature = "other-platform"))]
        assert!(matches!(
            App::new("foo-drive@1.0.0"),
            Err(ParseAppVersionErr::Name(ParseAppNameErr::Platform(_)))
        ));
    }

    #[cfg(feature = "other-product")]
    #[test]
    fn test_product_other_display() {
        assert_eq!(
            format!("{}", Product::from_str("someproduct").unwrap()),
            "someproduct"
        );
    }

    #[cfg(feature = "other-platform")]
    #[test]
    fn test_platform_other_display() {
        assert_eq!(
            format!("{}", Platform::from_str("someplatform").unwrap()),
            "someplatform"
        );
    }
}
