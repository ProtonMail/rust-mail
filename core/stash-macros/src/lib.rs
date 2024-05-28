//! Macros for the `stash` crate.
//!
//! The macros implemented in this crate are proc macros, which have to live
//! separately from other code. They are part of the `stash` crate's ecosystem.
//!

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Data, DeriveInput, Error as SynError, Fields, Ident, LitStr, Path};

/// Automatically derive the `DbRecord` trait for a struct.
///
/// The `DbRecord` trait is used to define a mapping between a struct and a
/// simple database record. This macro generates an implementation of the
/// `DbRecord` trait for the annotated struct.
///
/// It is important to include the following attributes on the struct:
///
///   - `#[DbField]`: Any other field that should be included in the database
///     record. These can be of any type supported for (de)serialisation from/to
///     `rusqlite`.
///
/// The `DbField` attribute can also be used to specify a wrapper type for the
/// field. This is useful when the field type does not directly implement the
/// `ToSql` and `FromSql` traits. In this case, the attribute should be used as
/// `#[DbField(WrapperType)]`, where `WrapperType` is the type that should be
/// used to wrap the field for database operations. The wrapper type should
/// implement the `From` trait for the field type, and the `ToSql` and `FromSql`
/// traits for the database operations.
///
/// # Example
///
/// ```rust
/// use serde::{Serialize, Deserialize};
/// use stash::macros::DbRecord;
/// use stash::orm::{CsvArray, DbRecord};
///
/// #[derive(Clone, Debug, DbRecord, Deserialize, PartialEq, Serialize)]
/// struct Foo {
///     #[DbField]
///     name: String,
///
///     #[DbField]
///     value: i32,
///
///     #[DbField(via CsvArray<i32>)]
///     values: Vec<i32>,
/// }
/// ```
///
#[proc_macro_derive(DbRecord, attributes(DbField))]
pub fn db_record_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Extract attributes
    let fields = if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            fields.named.iter().collect::<Vec<_>>()
        } else {
            panic!("DbRecord can only be derived for structs with named fields")
        }
    } else {
        panic!("DbRecord can only be derived for structs")
    };

    let db_fields: Vec<Ident> = fields
        .iter()
        .filter_map(|field| {
            if field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("DbField"))
            {
                field.ident.clone()
            } else {
                None
            }
        })
        .collect();

    let via_attrs: Vec<Option<ViaIntermediary>> = fields
        .iter()
        .map(|field| {
            field
                .attrs
                .iter()
                .find(|attr| attr.path().is_ident("DbField"))
                .and_then(|attr| match attr.parse_args::<ViaIntermediary>() {
                    Ok(via) => Some(via),
                    Err(err) => {
                        if err
                            .to_string()
                            .contains("expected attribute arguments in parentheses")
                        {
                            None
                        } else {
                            panic!("Failed to parse attribute: {}", err)
                        }
                    }
                })
        })
        .collect();

    // Generate trait implementation
    let fields_impl = db_fields
        .iter()
        .zip(via_attrs.iter())
        .map(|(db_field, via_attr)| {
            if let Some(via_type) = via_attr {
                quote! {
                    Box::new(<#via_type as From<_>>::from(self.#db_field.clone()))
                }
            } else {
                quote! {
                    Box::new(self.#db_field.clone())
                }
            }
        });

    let field_values_impl = db_fields
        .iter()
        .zip(via_attrs.iter())
        .map(|(db_field, via_attr)| {
            if let Some(via_type) = via_attr {
                quote! {
                    Box::new(<#via_type as From<_>>::from(self.#db_field.clone()))
                }
            } else {
                quote! {
                    Box::new(self.#db_field.clone())
                }
            }
        });

    let from_row_impl = fields.iter().zip(via_attrs.iter()).map(|(field, via_attr)| {
        let field_ident = field.ident.as_ref().expect("All fields must have an identifier");
        if let Some(via_type) = via_attr {
            quote! {
                <#via_type as Into<_>>::into(row.get(
                    columns.iter().position(|c| c == stringify!(#field_ident))
                        .ok_or_else(|| stash::orm::ConversionError::MissingColumn(stringify!(#field_ident).to_owned()))?
                )?)
            }
        } else {
            quote! {
                row.get(
                    columns.iter().position(|c| c == stringify!(#field_ident))
                        .ok_or_else(|| stash::orm::ConversionError::MissingColumn(stringify!(#field_ident).to_owned()))?
                )?
            }
        }
    });

    (quote! {
        impl stash::orm::DbRecord for #name {
            fn fields(&self) -> std::collections::HashMap<&'static str, Box<dyn stash::exports::ToSql + Send>> {
                let mut map = std::collections::HashMap::new();
                #(
                    map.insert(stringify!(#db_fields), #fields_impl as Box<dyn stash::exports::ToSql + Send>);
                )*
                map
            }

            fn field_names() -> Vec<&'static str> {
                vec![#(stringify!(#db_fields)),*]
            }

            fn field_values(&self) -> Vec<Box<dyn stash::exports::ToSql + Send>> {
                vec![
                    #(#field_values_impl as Box<dyn stash::exports::ToSql + Send>),*
                ]
            }

            fn from_row(row: &stash::exports::Row, columns: &[String]) -> Result<Self, stash::orm::ConversionError> {
                Ok(Self {
                    #(
                        #db_fields: #from_row_impl,
                    )*
                })
            }
        }
    }).into()
}

/// Automatically derive the `Model` trait for a struct.
///
/// The `Model` trait is used to define a mapping between a struct and a
/// fully-modelled database record. This macro generates an implementation of
/// the `DbRecord` trait for the annotated struct.
///
/// It is important to include the following attributes on the struct:
///
///   - `#[TableName("table_name")]`: The name of the table in the database.
///     Notably, this is applied as a struct-level annotation, rather than being
///     applied to a struct field.
///   - `#[StashField]`: The field that contains the associated `Stash` for the
///     record. Note that it is important to apply `#[serde(skip)]` to this
///     field to avoid it being included in the serialisation requirements.
///   - `#[IdField]`: The field that contains the primary key for the record.
///     This is expected to be a `Uuid` field.
///   - `#[DbField]`: Any other field that should be included in the database
///     record. These can be of any type supported for (de)serialisation from/to
///     `rusqlite`.
///
/// The `DbField` attribute can also be used to specify a wrapper type for the
/// field. This is useful when the field type does not directly implement the
/// `ToSql` and `FromSql` traits. In this case, the attribute should be used as
/// `#[DbField(WrapperType)]`, where `WrapperType` is the type that should be
/// used to wrap the field for database operations. The wrapper type should
/// implement the `From` trait for the field type, and the `ToSql` and `FromSql`
/// traits for the database operations.
///
/// # Example
///
/// ```rust
/// use serde::{Serialize, Deserialize};
/// use stash::macros::Model;
/// use stash::orm::{CsvArray, Model};
/// use stash::stash::Stash;
/// use uuid::Uuid;
///
/// #[derive(Clone, Debug, Model, Deserialize, PartialEq, Serialize)]
/// #[TableName("foo_table")]
/// struct Foo {
///     #[IdField]
///     id: Uuid,
///
///     #[DbField]
///     name: String,
///
///     #[DbField]
///     value: i32,
///
///     #[DbField(via CsvArray<i32>)]
///     values: Vec<i32>,
///
///     #[StashField]
///     #[serde(skip)]
///     stash: Option<Stash>,
/// }
/// ```
///
#[proc_macro_derive(Model, attributes(DbField, IdField, StashField, TableName))]
pub fn model_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    // Extract attributes
    let table_name = input
        .attrs
        .iter()
        .find_map(|attr| {
            if attr.path().is_ident("TableName") {
                attr.parse_args::<LitStr>().ok()
            } else {
                None
            }
        })
        .expect("TableName attribute is missing");

    let fields = if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            fields.named.iter().collect::<Vec<_>>()
        } else {
            panic!("Model can only be derived for structs with named fields")
        }
    } else {
        panic!("Model can only be derived for structs")
    };

    let id_field = fields
        .iter()
        .find(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("IdField"))
        })
        .expect("IdField attribute is missing")
        .ident
        .as_ref()
        .expect("IdField must have an identifier");

    let id_type = &fields
        .iter()
        .find(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("IdField"))
        })
        .expect("IdField attribute is missing")
        .ty;

    let stash_field = fields
        .iter()
        .find(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("StashField"))
        })
        .expect("StashField attribute is missing")
        .ident
        .as_ref()
        .expect("StashField must have an identifier");

    let db_fields: Vec<Ident> = fields
        .iter()
        .filter_map(|field| {
            if field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("DbField"))
                || field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("IdField"))
            {
                field.ident.clone()
            } else {
                None
            }
        })
        .collect();

    let via_attrs: Vec<Option<ViaIntermediary>> = fields
        .iter()
        .map(|field| {
            field
                .attrs
                .iter()
                .find(|attr| attr.path().is_ident("DbField") || attr.path().is_ident("IdField"))
                .and_then(|attr| match attr.parse_args::<ViaIntermediary>() {
                    Ok(via) => Some(via),
                    Err(err) => {
                        if err
                            .to_string()
                            .contains("expected attribute arguments in parentheses")
                        {
                            None
                        } else {
                            panic!("Failed to parse attribute: {}", err)
                        }
                    }
                })
        })
        .collect();

    // Generate trait implementation
    let fields_impl = db_fields
        .iter()
        .zip(via_attrs.iter())
        .map(|(db_field, via_attr)| {
            if let Some(via_type) = via_attr {
                quote! {
                    Box::new(<#via_type as From<_>>::from(self.#db_field.clone()))
                }
            } else {
                quote! {
                    Box::new(self.#db_field.clone())
                }
            }
        });

    let field_values_impl = db_fields
        .iter()
        .zip(via_attrs.iter())
        .map(|(db_field, via_attr)| {
            if let Some(via_type) = via_attr {
                quote! {
                    Box::new(<#via_type as From<_>>::from(self.#db_field.clone()))
                }
            } else {
                quote! {
                    Box::new(self.#db_field.clone())
                }
            }
        });

    let from_row_impl = fields.iter().zip(via_attrs.iter()).map(|(field, via_attr)| {
        let field_ident = field.ident.as_ref().expect("All fields must have an identifier");
        if let Some(via_type) = via_attr {
            quote! {
                <#via_type as Into<_>>::into(row.get(
                    columns.iter().position(|c| c == stringify!(#field_ident))
                        .ok_or_else(|| stash::orm::ConversionError::MissingColumn(stringify!(#field_ident).to_owned()))?
                )?)
            }
        } else {
            quote! {
                row.get(
                    columns.iter().position(|c| c == stringify!(#field_ident))
                        .ok_or_else(|| stash::orm::ConversionError::MissingColumn(stringify!(#field_ident).to_owned()))?
                )?
            }
        }
    });

    (quote! {
        impl stash::orm::DbRecord for #name {
            fn fields(&self) -> std::collections::HashMap<&'static str, Box<dyn stash::exports::ToSql + Send>> {
                let mut map = std::collections::HashMap::new();
                #(
                    map.insert(stringify!(#db_fields), #fields_impl as Box<dyn stash::exports::ToSql + Send>);
                )*
                map
            }

            fn field_names() -> Vec<&'static str> {
                vec![#(stringify!(#db_fields)),*]
            }

            fn field_values(&self) -> Vec<Box<dyn stash::exports::ToSql + Send>> {
                vec![
                    #(#field_values_impl as Box<dyn stash::exports::ToSql + Send>),*
                ]
            }

            fn from_row(row: &stash::exports::Row, columns: &[String]) -> Result<Self, stash::orm::ConversionError> {
                Ok(Self {
                    #(
                        #db_fields: #from_row_impl,
                    )*
                    #stash_field: None,
                })
            }
        }

        impl stash::orm::Model for #name {
            type Id = #id_type;

            fn id(&self) -> Self::Id {
                self.#id_field.clone()
            }

            fn id_field_name() -> &'static str {
                stringify!(#id_field)
            }

            fn stash(&self) -> &stash::stash::Stash {
                &self.#stash_field.as_ref().expect("Stash field is not set")
            }

            fn set_stash(&mut self, stash: &stash::stash::Stash) {
                self.#stash_field = Some(stash.clone());
            }

            fn table_name() -> &'static str {
                #table_name
            }
        }
    }).into()
}

/// Details of the `via` attribute.
///
/// This struct is used to parse the `via` attribute, which is used to specify
/// a wrapper type for a field in the `DbRecord` and `Model` derive macros.
///
#[derive(Debug)]
struct ViaIntermediary(Option<Path>);

impl Parse for ViaIntermediary {
    fn parse(input: ParseStream) -> Result<Self, SynError> {
        if input.is_empty() {
            return Ok(ViaIntermediary(None));
        }
        let arg: Ident = input.parse()?;
        if arg != "via" {
            return Err(SynError::new(arg.span(), "expected `via`"));
        }
        Ok(ViaIntermediary(Some(input.parse()?)))
    }
}

impl ToTokens for ViaIntermediary {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        if let Some(identifier) = &self.0 {
            tokens.extend(quote! {
                #identifier
            });
        }
    }
}
