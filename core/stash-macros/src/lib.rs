//! Macros for the `stash` crate.
//!
//! The macros implemented in this crate are proc macros, which have to live
//! separately from other code. They are part of the `stash` crate's ecosystem.
//!

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::{
    parse_macro_input, Data, DeriveInput, Error as SynError, Field, Fields, Ident, LitStr, Path,
    Type,
};

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
    let fields = extract_fields(&input, "DbRecord");
    let db_fields = extract_db_fields(&fields, false);
    let via_attrs = extract_via_attrs(&fields, false);

    // Generate trait implementation
    let db_fields_impl = generate_db_field_values_impl(&db_fields, &via_attrs);
    let db_field_values_impl = db_fields_impl.clone();
    let from_row_values_impl = generate_from_row_values_impl(&fields, &via_attrs);
    let fn_fields_impl = generate_fn_fields_impl(&db_fields, &db_fields_impl);
    let fn_field_names_impl = generate_fn_field_names_impl(&db_fields);
    let fn_field_values_impl = generate_fn_field_values_impl(&db_field_values_impl);
    let fn_from_row_impl = generate_fn_from_row_impl(&db_fields, &from_row_values_impl, None);

    (quote! {
        impl stash::orm::DbRecord for #name {
            #fn_fields_impl
            #fn_field_names_impl
            #fn_field_values_impl
            #fn_from_row_impl
        }
    })
    .into()
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
    let table_name = extract_table_name(&input);
    let fields = extract_fields(&input, "Model");
    let (id_field, id_type) = extract_id_field(&fields);
    let stash_field = extract_stash_field(&fields);
    let db_fields = extract_db_fields(&fields, true);
    let via_attrs = extract_via_attrs(&fields, true);

    // Generate trait implementation
    let db_fields_impl = generate_db_field_values_impl(&db_fields, &via_attrs);
    let db_field_values_impl = db_fields_impl.clone();
    let from_row_values_impl = generate_from_row_values_impl(&fields, &via_attrs);
    let fn_fields_impl = generate_fn_fields_impl(&db_fields, &db_fields_impl);
    let fn_field_names_impl = generate_fn_field_names_impl(&db_fields);
    let fn_field_values_impl = generate_fn_field_values_impl(&db_field_values_impl);
    let fn_from_row_impl =
        generate_fn_from_row_impl(&db_fields, &from_row_values_impl, Some(&stash_field));

    (quote! {
        impl stash::orm::DbRecord for #name {
            #fn_fields_impl
            #fn_field_names_impl
            #fn_field_values_impl
            #fn_from_row_impl
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
    })
    .into()
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

/// Extract the fields that should be included in the database record.
///
/// This function extracts the fields that should be included in the database
/// record from the struct fields. It filters out any fields that do not have
/// the `DbField` attribute, optionally also including the field with the
/// `IdField` attribute.
///
/// # Parameters
///
/// * `fields`           - The fields of the struct. Specifically, *all* the
///                        fields that the struct has.
/// * `include_id_field` - Allow an `IdField` in addition to `DbField`s.
///
fn extract_db_fields(fields: &[&Field], include_id_field: bool) -> Vec<Ident> {
    fields
        .iter()
        .filter_map(|field| {
            if field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("DbField"))
                || (include_id_field
                    && field
                        .attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("IdField")))
            {
                field.ident.clone()
            } else {
                None
            }
        })
        .collect()
}

/// Extract the fields of a struct.
///
/// This function extracts the fields of a struct, ensuring that the struct is
/// indeed a struct with named fields.
///
/// # Parameters
///
/// * `input`      - The input from the derive macro, which should be a struct.
/// * `macro_name` - The name of the macro that is being derived.
///
/// # Panics
///
/// This function panics if the input is not a struct with named fields.
///
fn extract_fields<'a>(input: &'a DeriveInput, macro_name: &'a str) -> Vec<&'a Field> {
    if let Data::Struct(data) = &input.data {
        if let Fields::Named(fields) = &data.fields {
            fields.named.iter().collect::<Vec<_>>()
        } else {
            panic!("{macro_name} can only be derived for structs with named fields")
        }
    } else {
        panic!("{macro_name} can only be derived for structs")
    }
}

/// Extract the field that is marked as the primary key.
///
/// This function extracts the field that is marked as the primary key from the
/// struct fields. It is expected that there is exactly one field marked with
/// the `IdField` attribute.
///
/// # Parameters
///
/// * `fields` - The fields of the struct. Specifically, *all* the fields that
///              the struct has.
///
/// # Panics
///
/// This function panics if the `IdField` attribute is missing, or does not have
/// an identifier.
///
fn extract_id_field(fields: &[&Field]) -> (Ident, Type) {
    let id_field = fields
        .iter()
        .find(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("IdField"))
        })
        .expect("IdField attribute is missing");
    (
        id_field
            .ident
            .clone()
            .expect("IdField must have an identifier"),
        id_field.ty.clone(),
    )
}

/// Extract the field that is marked as the `stash` field.
///
/// This function extracts the field that is marked as the `stash` field from
/// the struct fields. It is expected that there is exactly one field marked
/// with the `StashField` attribute.
///
/// The `stash` field is the field that contains the associated `Stash` for the
/// record.
///
/// # Parameters
///
/// * `fields` - The fields of the struct. Specifically, *all* the fields that
///              the struct has.
///
/// # Panics
///
/// This function panics if the `StashField` attribute is missing, or does not
/// have an identifier.
///
fn extract_stash_field(fields: &[&Field]) -> Ident {
    fields
        .iter()
        .find(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("StashField"))
        })
        .expect("StashField attribute is missing")
        .ident
        .clone()
        .expect("StashField must have an identifier")
}

/// Extract the table name from the struct attributes.
///
/// This function extracts the table name from the struct attributes. It is
/// expected that there is exactly one attribute with the name `TableName`.
///
/// # Parameters
///
/// * `input` - The input from the derive macro, which should be a struct.
///
/// # Panics
///
/// This function panics if the `TableName` attribute is missing.
///
fn extract_table_name(input: &DeriveInput) -> LitStr {
    input
        .attrs
        .iter()
        .find_map(|attr| {
            if attr.path().is_ident("TableName") {
                attr.parse_args::<LitStr>().ok()
            } else {
                None
            }
        })
        .expect("TableName attribute is missing")
}

/// Extract attributes with a `via` argument from the struct fields.
///
/// This function extracts the attributes with a `via` argument from the struct
/// fields. It is expected that the optional `via` argument is used to specify a
/// wrapper type for the field, for those that require it.
///
/// # Parameters
///
/// * `fields`           - The fields of the struct. Specifically, *all* the
///                        fields that the struct has.
/// * `include_id_field` - Allow an `IdField` in addition to `DbField`s.
///
/// # Panics
///
/// This function panics if the attribute cannot be parsed.
///
fn extract_via_attrs(fields: &[&Field], include_id_field: bool) -> Vec<Option<ViaIntermediary>> {
    fields
        .iter()
        .map(|field| {
            field
                .attrs
                .iter()
                .find(|attr| {
                    attr.path().is_ident("DbField")
                        || (include_id_field && attr.path().is_ident("IdField"))
                })
                .and_then(|attr| match attr.parse_args::<ViaIntermediary>() {
                    Ok(via) => Some(via),
                    Err(err) => {
                        if err
                            .to_string()
                            .contains("expected attribute arguments in parentheses")
                        {
                            None
                        } else {
                            panic!("Failed to parse attribute: {err}")
                        }
                    }
                })
        })
        .collect()
}

/// Generate code implementation for individual database field values.
///
/// This function generates the code implementation to return the values of
/// individual database fields. These are returned in a form that is compatible
/// with conversion to SQL type, but pre-conversion.
///
/// Note: Any fields using an intermediary type (i.e. specified with the `via`
/// attribute argument) will be converted to that type before being returned.
///
/// # Parameters
///
/// * `db_fields` - The list of database fields for which values should be
///                 generated.
/// * `via_attrs` - The `via` attributes for the fields. If specified for a
///                 field, that field will be wrapped in the intermediary type.
///
fn generate_db_field_values_impl(
    db_fields: &[Ident],
    via_attrs: &[Option<ViaIntermediary>],
) -> Vec<TokenStream2> {
    db_fields
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
        })
        .collect()
}

/// Generate code implementation for the `fields()` method.
///
/// # Parameters
///
/// * `db_fields`            - The list of database fields.
/// * `db_field_values_impl` - The code implementation for the database field
///                            values.
///
fn generate_fn_fields_impl(
    db_fields: &[Ident],
    db_field_values_impl: &[TokenStream2],
) -> TokenStream2 {
    quote! {
        fn fields(&self) -> std::collections::HashMap<&'static str, Box<dyn stash::exports::ToSql + Send>> {
            let mut map = std::collections::HashMap::new();
            #(
                map.insert(stringify!(#db_fields), #db_field_values_impl as Box<dyn stash::exports::ToSql + Send>);
            )*
            map
        }
    }
}

/// Generate code implementation for the `field_names()` method.
///
/// # Parameters
///
/// * `db_fields` - The list of database fields.
///
fn generate_fn_field_names_impl(db_fields: &[Ident]) -> TokenStream2 {
    quote! {
        fn field_names() -> Vec<&'static str> {
            vec![#(stringify!(#db_fields)),*]
        }
    }
}

/// Generate code implementation for the `field_values()` method.
///
/// # Parameters
///
/// * `db_field_values_impl` - The code implementation for the database field
///                            values.
///
fn generate_fn_field_values_impl(db_field_values_impl: &[TokenStream2]) -> TokenStream2 {
    quote! {
        fn field_values(&self) -> Vec<Box<dyn stash::exports::ToSql + Send>> {
            vec![
                #(#db_field_values_impl as Box<dyn stash::exports::ToSql + Send>),*
            ]
        }
    }
}

/// Generate code implementation for the `from_row()` method.
///
/// # Parameters
///
/// * `db_fields`            - The list of database fields.
/// * `from_row_values_impl` - The code implementation to convert the values
///                            from the database row to the appropriate struct
///                            field types.
/// * `stash_field`          - The field that contains the associated `Stash`.
///                            If [`None`], then this will be excluded from the
///                            generated code.
///
fn generate_fn_from_row_impl(
    db_fields: &[Ident],
    from_row_values_impl: &[TokenStream2],
    stash_field: Option<&Ident>,
) -> TokenStream2 {
    let stash_field_impl = if let Some(stash_field) = stash_field {
        quote! {
            #stash_field: Some(stash.clone()),
        }
    } else {
        quote! {}
    };

    quote! {
        fn from_row(row: &stash::exports::Row, columns: &[String], stash: stash::stash::Stash) -> Result<Self, stash::orm::ConversionError> {
            Ok(Self {
                #(
                    #db_fields: #from_row_values_impl,
                )*
                #stash_field_impl
            })
        }
    }
}

/// Generate code implementation to convert individual field values.
///
/// This function generates the code implementation to convert individual field
/// values from the database row, i.e. from SQL types, to the appropriate struct
/// field types.
///
/// # Parameters
///
/// * `fields`     - The fields of the struct. Specifically, *all* the fields
///                  that the struct has.
/// * `via_attrs`  - The `via` attributes for the fields. If specified for a
///                  field, that field will be converted from a SQL field type
///                  to the intermediary type before converting to the struct
///                  field type.
///
/// # Panics
///
/// This function panics if any fields do not have an identifier.
///
fn generate_from_row_values_impl(
    fields: &[&Field],
    via_attrs: &[Option<ViaIntermediary>],
) -> Vec<TokenStream2> {
    fields.iter().zip(via_attrs.iter()).map(|(field, via_attr)| {
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
    })
    .collect()
}
