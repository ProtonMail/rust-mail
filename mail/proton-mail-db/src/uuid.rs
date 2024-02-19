#[macro_export]
macro_rules! new_uuid_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Eq, PartialEq, Hash)]
        pub struct $name(uuid::Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl From<uuid::Uuid> for $name {
            fn from(value: uuid::Uuid) -> Self {
                Self(value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }

        #[allow(unused_qualifications)]
        impl proton_sqlite3::rusqlite::types::FromSql for $name {
            fn column_result(
                value: proton_sqlite3::rusqlite::types::ValueRef<'_>,
            ) -> proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
                uuid::Uuid::column_result(value).map($name)
            }
        }

        #[allow(unused_qualifications)]
        impl proton_sqlite3::rusqlite::types::ToSql for $name {
            fn to_sql(
                &self,
            ) -> proton_sqlite3::rusqlite::Result<proton_sqlite3::rusqlite::types::ToSqlOutput<'_>>
            {
                self.0.to_sql()
            }
        }
    };
}

#[macro_export]
macro_rules! new_u64_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
        pub struct $name(u64);

        impl $name {
            pub fn new(v: u64) -> Self {
                Self(v)
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }

        #[allow(unused_qualifications)]
        impl proton_sqlite3::rusqlite::types::FromSql for $name {
            fn column_result(
                value: proton_sqlite3::rusqlite::types::ValueRef<'_>,
            ) -> proton_sqlite3::rusqlite::types::FromSqlResult<Self> {
                u64::column_result(value).map($name)
            }
        }

        #[allow(unused_qualifications)]
        impl proton_sqlite3::rusqlite::types::ToSql for $name {
            fn to_sql(
                &self,
            ) -> proton_sqlite3::rusqlite::Result<proton_sqlite3::rusqlite::types::ToSqlOutput<'_>>
            {
                self.0.to_sql()
            }
        }
    };
}

pub use new_u64_type;
