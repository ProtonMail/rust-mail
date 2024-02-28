/// Bind a very large number of parameters that exceed the current parameter capacity of
/// rusqlite. You can execute the statement by hand later.
/// ```ignore
/// let mut stmt = connection.prepare(...);
/// bind_list!(&mut stmt,
/// param1,
/// param2,
/// ...,
/// paramN);
///
#[macro_export]
macro_rules! bind_list_indexed {
    ($stmt:expr, $($exp:expr,)+ $(,)?) => {
        bind_list_indexed_recursive!(1, $stmt, $($exp),+);
    };
}

#[macro_export]
macro_rules! bind_list_indexed_recursive {
    ($index:tt, $stmt:expr, $exp:expr $(,)?) => {
        $stmt.raw_bind_parameter($index,$exp)?;
    };

    ($index:tt, $stmt:expr, $exp:expr $(,$r:expr)+) => {
        $stmt.raw_bind_parameter($index, $exp)?;
        bind_list_indexed_recursive!(($index+1),$stmt $(,$r)+)
    };
}

#[macro_export]
macro_rules! new_tracked_connection_wrapper {
    ($name:ident) => {
        $crate::paste::paste! {
            pub struct $name(pub(crate) $crate::TrackingConnection);

            impl $name {
                pub fn new(conn:$crate::TrackingConnection) -> Self {
                    Self(conn)
                }

                /// Get access to read only connection implementations.
                pub fn as_connection_ref(&self) -> [<$name Ref>]<'_> {
                    [<$name Ref>]([<$name Impl>](self.0.as_ref()))
                }

                /// Convert the current into another connection type generated from this macro.
                pub fn into_connection_wrapper<T:From<$crate::TrackingConnection>>(self) -> T{
                    T::from(self.0)
                }

                /// Create a new transaction.
                pub fn tx<T, E: From<$crate::rusqlite::Error>>(
                    &mut self,
                    mut closure: impl FnMut(&mut [<$name Mut>]) -> Result<T, E>,
                ) -> Result<T, E> {
                    self.0.tx(|tx| {
                        let conn_impl = [<$name Impl>](tx.deref());
                        let mut conn = [<$name Mut>](conn_impl);
                        closure(&mut conn)
                    })
                }
            }

            impl From<$crate::TrackingConnection> for $name {
                fn from(value: $crate::TrackingConnection) -> Self {
                    Self::new(value)
                }
            }


            pub struct [<$name Impl>]<'c>(pub(crate) &'c $crate::rusqlite::Connection);

            impl<'c> [<$name Impl>]<'c> {
                pub fn new(conn: &'c $crate::rusqlite::Connection) -> Self {
                    Self(conn)
                }
            }

            impl<'c> From<&'c $crate::rusqlite::Connection> for [<$name Impl>]<'c> {
                fn from(value: &'c $crate::rusqlite::Connection) -> Self {
                    Self::new(value)
                }
            }

            /// Wrapper to promote read only access.
            pub struct [<$name Ref>]<'c>([<$name Impl>]<'c>);

            impl<'c> std::ops::Deref for [<$name Ref>]<'c> {
                type Target = [<$name Impl>]<'c>;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            /// Wrapper to promote read and write.
            pub struct [<$name Mut>]<'c>([<$name Impl>]<'c>);

            impl<'c> std::ops::Deref for [<$name Mut>]<'c> {
                type Target = [<$name Impl>]<'c>;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl<'c> std::ops::DerefMut for [<$name Mut>]<'c> {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.0
                }
            }
        }
    };
}

#[macro_export]
macro_rules! new_connection_wrapper {
    ($name:ident) => {
        $crate::paste::paste! {
            pub struct $name(pub(crate) $crate::SqliteConnection);

            impl $name {
                pub fn new(conn:$crate::SqliteConnection) -> Self {
                    Self(conn)
                }

                /// Get access to read only connection implementations.
                pub fn as_connection_ref(&self) -> [<$name Ref>]<'_> {
                    [<$name Ref>]([<$name Impl>](&self.0))
                }

                /// Convert the current into another connection type generated from this macro.
                pub fn into_connection_wrapper<T:From<$crate::SqliteConnection>>(self) -> T{
                    T::from(self.0)
                }

                /// Create a new transaction.
                pub fn tx<T, E: From<$crate::rusqlite::Error>>(
                    &mut self,
                    mut closure: impl FnMut(&mut [<$name Mut>]) -> Result<T, E>,
                ) -> Result<T, E> {
                    self.0.tx(|tx| {
                        let conn_impl = [<$name Impl>](tx.deref());
                        let mut conn = [<$name Mut>](conn_impl);
                        closure(&mut conn)
                    })
                }
            }

            impl From<$crate::SqliteConnection> for $name {
                fn from(value: $crate::SqliteConnection) -> Self {
                    Self::new(value)
                }
            }

            pub struct [<$name Impl>]<'c>(pub(crate) &'c $crate::rusqlite::Connection);

            impl<'c> [<$name Impl>]<'c> {
                pub fn new(conn: &'c $crate::rusqlite::Connection) -> Self {
                    Self(conn)
                }
            }

            impl<'c> From<&'c $crate::rusqlite::Connection> for [<$name Impl>]<'c> {
                fn from(value: &'c $crate::rusqlite::Connection) -> Self {
                    Self::new(value)
                }
            }

            /// Wrapper to promote read only access.
            pub struct [<$name Ref>]<'c>([<$name Impl>]<'c>);

            impl<'c> std::ops::Deref for [<$name Ref>]<'c> {
                type Target = [<$name Impl>]<'c>;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            /// Wrapper to promote read and write.
            pub struct [<$name Mut>]<'c>([<$name Impl>]<'c>);

            impl<'c> std::ops::Deref for [<$name Mut>]<'c> {
                type Target = [<$name Impl>]<'c>;

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl<'c> std::ops::DerefMut for [<$name Mut>]<'c> {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.0
                }
            }
        }
    };
}
