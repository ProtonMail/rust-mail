use alloc::string::String;

/// Holds both the short and full type names.
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeNameInfo {
    /// The short type name, e.g. `"Option<String>"`.
    pub short_name: String,
    /// The full type name, e.g. `"core::option::Option<alloc::string::String>"`.
    pub full_name: String,
}

impl TypeNameInfo {
    /// Returns `TypeNameInfo` for the given `T`.
    pub fn new<T>() -> Self {
        let short_name = crate::type_name::<T>();
        let full_name = String::from(core::any::type_name::<T>());

        Self {
            short_name,
            full_name,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::String};

    use super::TypeNameInfo;

    #[test]
    fn clone() {
        let type_name_info = TypeNameInfo::new::<Option<String>>();

        let clone = Clone::clone(&type_name_info);

        assert_eq!(type_name_info, clone);
    }

    #[test]
    fn debug() {
        let type_name_info = TypeNameInfo::new::<Option<String>>();

        assert_eq!(
            "TypeNameInfo { \
                short_name: \"Option<String>\", \
                full_name: \"core::option::Option<alloc::string::String>\" \
            }",
            format!("{type_name_info:?}")
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serialize() -> Result<(), serde_yaml::Error> {
        let type_name_info = TypeNameInfo::new::<Option<String>>();

        let serialized = serde_yaml::to_string(&type_name_info)?;

        assert_eq!(
            "\
                short_name: Option<String>\n\
                full_name: core::option::Option<alloc::string::String>\n\
            ",
            serialized
        );

        Ok(())
    }

    #[cfg(feature = "serde")]
    #[test]
    fn deserialize() -> Result<(), serde_yaml::Error> {
        let deserialized = serde_yaml::from_str(
            "\
                short_name: Option<String>\n\
                full_name: core::option::Option<alloc::string::String>\n\
            ",
        )?;

        assert_eq!(TypeNameInfo::new::<Option<String>>(), deserialized);

        Ok(())
    }
}
