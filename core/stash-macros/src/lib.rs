//! Macros for the `stash` crate.
//!
//! The macros implemented in this crate are proc macros, which have to live
//! separately from other code. They are part of the `stash` crate's ecosystem.
//!

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Data, DeriveInput, Error as SynError, Field, Fields, Ident, LitStr, Path, Token, Type,
    parse_macro_input,
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
/// use stash::orm::DbRecord;
///
/// #[derive(Clone, Debug, DbRecord, Deserialize, PartialEq, Serialize)]
/// struct Foo {
///     #[DbField]
///     name: String,
///
///     #[DbField]
///     value: i32,
/// }
/// ```
///
#[proc_macro_derive(DbRecord, attributes(DbField))]
pub fn db_record_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Extract attributes
    let fields = extract_fields(&input, "DbRecord");
    let db_fields = extract_db_fields(&fields, false);
    let internal_fields = None;
    let via_attrs = extract_via_attrs(&fields, false);

    // Generate trait implementation
    let db_fields_impl = generate_db_field_values_impl(&db_fields, &via_attrs);
    let db_field_values_impl = db_fields_impl.clone();
    let from_row_values_impl = generate_from_row_values_impl(&db_fields, &via_attrs);
    let fn_field_names_impl = generate_fn_field_names_impl(&db_fields);
    let fn_field_values_impl = generate_fn_field_values_impl(&db_field_values_impl);
    let fn_from_row_impl =
        generate_fn_from_row_impl(&db_fields, &fields, internal_fields, &from_row_values_impl);

    (quote! {
        impl #impl_generics stash::orm::DbRecord for #name #ty_generics #where_clause {
            fn field_names() -> Vec<&'static str> {
                #fn_field_names_impl
            }

            fn field_values(&self) -> Vec<Box<dyn stash::exports::ToSql + Send>> {
                #fn_field_values_impl
            }

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
///   - `#[RowIdField]`: The field that contains the associated internal SQLite
///     row ID (a [`u64`] value) for the record. Note that it is important to
///     apply `#[serde(skip)]` to this field to avoid it being included in the
///     serialisation requirements. It is a special internal field, and not
///     included in the list of database fields.
///   - `#[IdField]`: The field that contains the primary key for the record.
///     This can be any type, as defined by the associated type [`Model::Id`].
///     If the field is `optional` or `autoincrement` then it will need to be
///     wrapped in an [`Option`] (see below).
///   - `#[DbField]`: Any other field that should be included in the database
///     record. These can be of any type supported for (de)serialisation from/to
///     `rusqlite`.
///
/// The `IdField` attribute has some configuration options available, namely
/// `autoincrement` and `optional`. The first supports automatic generation of
/// primary keys by the database, i.e. `AUTOINCREMENT`, and the second supports
/// general optionality for the field. In either case, the field will need to be
/// wrapped in an [`Option`].
///
/// The `DbField` attribute can also be used to specify a wrapper type for the
/// field. This is useful when the field type does not directly implement the
/// `ToSql` and `FromSql` traits. In this case, the attribute should be used as
/// `#[DbField(via WrapperType)]`, where `WrapperType` is the type that should
/// be used to wrap the field for database operations. The wrapper type should
/// implement the `From` trait for the field type, and the `ToSql` and `FromSql`
/// traits for the database operations.
///
/// # Customisation of actions
///
/// The `Model` trait implementation can be customised with additional actions
/// that are called when the model is loaded or saved to/from the database.
/// These actions can be defined by adding the `ModelActions` attribute to the
/// struct, with the actions specified as a comma-separated list. The available
/// actions are:
///
///   - `on_load`: This action is called when the model is loaded from the
///     database, and triggered by "load" and "find" operations.
///   - `on_save`: This action is called when the model is saved to the
///     database. It is triggered by "save" operations.
///
/// In both cases the custom action occurs after the normal operation has
/// been carried out.
///
/// # Examples
///
/// ## Example 1
///
/// ```rust
/// use serde::{Serialize, Deserialize};
/// use stash::macros::Model;
/// use stash::orm::Model;
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
///     #[RowIdField]
///     #[serde(skip)]
///     row_id: Option<u64>,
/// }
/// ```
///
/// ## Example 2
///
/// ```rust
/// use serde::{Serialize, Deserialize};
/// use stash::macros::Model;
/// use stash::orm::Model;
/// use stash::stash::Stash;
/// use uuid::Uuid;
///
/// #[derive(Clone, Debug, Model, Deserialize, PartialEq, Serialize)]
/// #[TableName("foo_table")]
/// struct Foo {
///     #[IdField(optional)]
///     id: Option<Uuid>,
///
///     #[DbField]
///     name: String,
///
///     #[DbField]
///     value: i32,
///
///     #[RowIdField]
///     #[serde(skip)]
///     row_id: Option<u64>,
/// }
/// ```
///
/// ## Example 3
///
/// ```rust
/// use serde::{Serialize, Deserialize};
/// use stash::macros::Model;
/// use stash::orm::Model;
/// use stash::stash::Stash;
/// use uuid::Uuid;
///
/// #[derive(Clone, Debug, Model, Deserialize, PartialEq, Serialize)]
/// #[TableName("foo_table")]
/// struct Foo {
///     #[IdField(autoincrement)]
///     id: Option<u64>,
///
///     #[DbField]
///     name: String,
///
///     #[DbField]
///     value: i32,
///
///
///     #[RowIdField]
///     #[serde(skip)]
///     row_id: Option<u64>,
/// }
/// ```
///
#[proc_macro_derive(
    Model,
    attributes(DbField, IdField, ModelActions, RowIdField, TableName)
)]
pub fn model_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Extract attributes
    let table_name = extract_table_name(&input);
    let (has_on_load, has_on_save) = extract_model_actions(&input);
    let fields = extract_fields(&input, "Model");
    let (id_field, id_type, is_optional, is_autoincrement) = extract_id_field(&fields);
    let row_id_field = extract_row_id_field(&fields);
    let db_fields = extract_db_fields(&fields, true);
    let db_fields_without_id = extract_db_fields(&fields, false);
    let internal_fields = Some(&row_id_field);
    let via_attrs = extract_via_attrs(&fields, true);
    let via_attrs_without_id = extract_via_attrs(&fields, false);

    // Generate trait implementation
    let id_field_type = if is_optional {
        quote! { Option<#id_type> }
    } else {
        quote! { #id_type }
    };

    let fn_id_impl = if is_optional {
        quote! { self.#id_field.clone().expect("called `id()` on a model that hasn't been saved yet") }
    } else {
        quote! { self.#id_field.clone() }
    };

    let db_fields_impl = generate_db_field_values_impl(&db_fields, &via_attrs);
    let db_fields_without_id_impl =
        generate_db_field_values_impl(&db_fields_without_id, &via_attrs_without_id);
    let db_field_values_impl = db_fields_impl.clone();
    let db_field_values_without_id_impl = db_fields_without_id_impl.clone();
    let from_row_values_impl = generate_from_row_values_impl(&db_fields, &via_attrs);
    let fn_field_names_impl = generate_fn_field_names_impl(&db_fields);
    let fn_field_values_impl = generate_fn_field_values_impl(&db_field_values_impl);
    let fn_field_names_without_id_impl = generate_fn_field_names_impl(&db_fields_without_id);
    let fn_field_values_without_id_impl =
        generate_fn_field_values_impl(&db_field_values_without_id_impl);
    let fn_from_row_impl =
        generate_fn_from_row_impl(&db_fields, &fields, internal_fields, &from_row_values_impl);
    let fn_id_value_impl = generate_fn_id_value_impl(&id_field, is_optional);
    let fn_set_id_value_impl = generate_fn_set_id_value_impl(&id_field, is_optional);

    // Generation custom action code
    let on_load_impl = if has_on_load {
        quote! {
            async fn find<Q>(
                query_logic: Q,
                params: Vec<Box<dyn stash::exports::ToSql + Send>>,
                tether: &stash::stash::Tether,
            ) -> Result<Vec<Self>, stash::stash::StashError>
            where
                Q: Into<String> + Send,
            {
                let mut instances = stash::orm::perform_find::<_, Self>(query_logic, params, tether).await?;
                for instance in instances.iter_mut() {
                    instance.on_load(tether).await?;
                }
                Ok(instances)
            }
            async fn find_first<Q>(
                query_logic: Q,
                params: Vec<Box<dyn stash::exports::ToSql + Send>>,
                tether: &stash::stash::Tether,
            ) -> Result<Option<Self>, stash::stash::StashError>
            where
                Q: Into<String> + Send,
            {
                let mut instance: Option<Self> = stash::orm::perform_find::<_, Self>(query_logic, params, &tether).await?.into_iter().next();
                match instance {
                    Some(mut i) => {
                        i.on_load(tether).await?;
                        Ok(Some(i))
                    },
                    None => Ok(None),
                }
            }
            async fn load(id: Self::IdType, tether: &stash::stash::Tether) -> Result<Option<Self>, stash::stash::StashError>
            {
                let mut instance: Option<Self> = stash::orm::perform_load(id, &tether).await?;
                match instance {
                    Some(mut i) => {
                        i.on_load(&tether).await?;
                        Ok(Some(i))
                    },
                    None => Ok(None),
                }
            }
        }
    } else {
        quote! {}
    };
    let on_save_impl = if has_on_save {
        quote! {
            async fn save(&mut self, bond: &stash::stash::Bond<'_>) -> Result<(), stash::stash::StashError>
            {
                stash::orm::perform_save(self, &bond).await?;
                self.on_save(&bond).await
            }
        }
    } else {
        quote! {}
    };

    (quote! {
        impl #impl_generics stash::orm::DbRecord for #name #ty_generics #where_clause {
            fn field_names() -> Vec<&'static str> {
                #fn_field_names_impl
            }

            fn field_values(&self) -> Vec<Box<dyn stash::exports::ToSql + Send>> {
                #fn_field_values_impl
            }

            #fn_from_row_impl
        }

        impl #impl_generics stash::orm::Model for #name #ty_generics #where_clause {
            type Id = #id_field_type;
            type IdType = #id_type;

            fn field_names_without_id() -> Vec<&'static str> {
                #fn_field_names_without_id_impl
            }

            fn field_values_without_id(&self) -> Vec<Box<dyn stash::exports::ToSql + Send>> {
                #fn_field_values_without_id_impl
            }

            fn id(&self) -> Self::IdType {
                #fn_id_impl
            }

            fn id_field_name() -> &'static str {
                stringify!(#id_field)
            }

            fn id_is_autoincrementing() -> bool {
                #is_autoincrement
            }

            fn id_is_optional() -> bool {
                #is_optional
            }

            fn id_value(&self) -> Result<Self::IdType, stash::stash::StashError> {
                #fn_id_value_impl
            }

            fn row_id(&self) -> Option<u64> {
                self.#row_id_field
            }

            fn set_id_value(&mut self, id: Self::IdType) {
                #fn_set_id_value_impl
            }

            fn set_row_id(&mut self, id: Option<u64>) {
                self.#row_id_field = id;
            }

            fn table_name() -> &'static str {
                #table_name
            }

            #on_load_impl
            #on_save_impl
        }
    })
    .into()
}

/// Details of the `via` attribute.
///
/// This struct is used to parse the `via` attribute, which is used to specify
/// a wrapper type for a field in the `DbRecord` and `Model` derive macros.
///
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
/// # Panics
///
/// This function panics if the `IdField` attribute is missing, or does not have
/// an identifier.
///
fn extract_id_field(fields: &[&Field]) -> (Ident, Type, bool, bool) {
    let id_field = fields
        .iter()
        .find(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("IdField"))
        })
        .expect("IdField attribute is missing");

    let mut is_optional = false;
    let mut is_autoincrement = false;

    for attr in &id_field.attrs {
        if attr.path().is_ident("IdField") {
            if let Ok(meta) = attr.parse_args_with(|input: ParseStream| {
                let mut args = Vec::new();
                while !input.is_empty() {
                    args.push(input.parse::<Ident>()?);
                    if input.is_empty() {
                        break;
                    }
                    input.parse::<Token![,]>()?;
                }
                Ok(args)
            }) {
                for arg in meta {
                    if arg == "optional" {
                        is_optional = true;
                    } else if arg == "autoincrement" {
                        is_optional = true;
                        is_autoincrement = true;
                    }
                }
            }
        }
    }

    let id_type = if is_optional {
        match &id_field.ty {
            Type::Path(type_path) if type_path.path.segments.last().unwrap().ident == "Option" => {
                if let syn::PathArguments::AngleBracketed(generic_args) =
                    &type_path.path.segments.last().unwrap().arguments
                {
                    if let syn::GenericArgument::Type(inner_type) =
                        generic_args.args.first().unwrap()
                    {
                        inner_type.clone()
                    } else {
                        panic!("Invalid IdField type: expected Option<T>");
                    }
                } else {
                    panic!("Invalid IdField type: expected Option<T>");
                }
            }
            _ => panic!("IdField with 'optional' or 'autoincrement' must be wrapped in an Option"),
        }
    } else {
        id_field.ty.clone()
    };

    (
        id_field
            .ident
            .clone()
            .expect("IdField must have an identifier"),
        id_type,
        is_optional,
        is_autoincrement,
    )
}

/// Extract the model actions from the struct attributes.
///
/// This function extracts the model actions from the struct attributes. It is
/// expected that there is no more than one attribute with the name
/// `ModelActions`, but it can be omitted.
///
/// # Panics
///
/// This function panics if the `TableName` attribute is missing.
///
fn extract_model_actions(input: &DeriveInput) -> (bool, bool) {
    let mut on_load = false;
    let mut on_save = false;

    for attr in &input.attrs {
        if attr.path().is_ident("ModelActions") {
            attr.parse_args_with(|input: ParseStream| {
                while !input.is_empty() {
                    let ident: Ident = input.parse()?;
                    match ident.to_string().as_str() {
                        "on_load" => on_load = true,
                        "on_save" => on_save = true,
                        _ => return Err(input.error("expected `on_load` or `on_save`")),
                    }
                    if input.is_empty() {
                        break;
                    }
                    input.parse::<Token![,]>()?;
                }
                Ok(())
            })
            .expect("Failed to parse ModelActions");
        }
    }

    (on_load, on_save)
}
/// Extract the field that is marked as the `row_id` field.
///
/// This function extracts the field that is marked as the `row_id` field from
/// the struct fields. It is expected that there is exactly one field marked
/// with the `RowIdField` attribute.
///
/// The `row_id` field is the field that contains the associated internal SQLite
/// row ID (a [`u64`]) for the record. This is not the same as the primary key
/// field represented by the `IdField` attribute.
///
/// # Panics
///
/// This function panics if the `RowIdField` attribute is missing, or does not
/// have an identifier.
///
fn extract_row_id_field(fields: &[&Field]) -> Ident {
    fields
        .iter()
        .find(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("RowIdField"))
        })
        .expect("RowIdField attribute is missing")
        .ident
        .clone()
        .expect("RowIdField must have an identifier")
}

/// Extract the table name from the struct attributes.
///
/// This function extracts the table name from the struct attributes. It is
/// expected that there is exactly one attribute with the name `TableName`.
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
/// Note that the `via` argument only applies to `DbField` attributes, and not
/// to `IdField` attributes.
///
/// # Panics
///
/// This function panics if the attribute cannot be parsed.
///
fn extract_via_attrs(fields: &[&Field], include_id_field: bool) -> Vec<Option<ViaIntermediary>> {
    fields
        .iter()
        .filter(|field| {
            field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("DbField"))
                || (include_id_field
                    && field
                        .attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("IdField")))
        })
        .map(|field| {
            if field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("IdField"))
            {
                None
            } else {
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
                                panic!("Failed to parse attribute: {err}")
                            }
                        }
                    })
            }
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

/// Generate code implementation for the `field_names()` method.
///
fn generate_fn_field_names_impl(db_fields: &[Ident]) -> TokenStream2 {
    quote! {
        vec![#(stringify!(#db_fields)),*]
    }
}

/// Generate code implementation for the `field_values()` method.
///
fn generate_fn_field_values_impl(db_field_values_impl: &[TokenStream2]) -> TokenStream2 {
    quote! {
        vec![
            #(#db_field_values_impl as Box<dyn stash::exports::ToSql + Send>),*
        ]
    }
}

/// Generate code implementation for the `from_row()` method.
///
/// # Parameters
///
/// * `internal_fields`      - Internal-used fields for `Model`s: firstly the
///   field that contains the internal row ID, and then
///   the field that contains the associated `Stash`.
///   If [`None`], then these will be excluded from the
///   generated code.
/// * `from_row_values_impl` - The code implementation to convert the values
///   from the database row to the appropriate struct
///   field types.
///
fn generate_fn_from_row_impl(
    db_fields: &[Ident],
    all_fields: &[&Field],
    internal_fields: Option<&Ident>,
    from_row_values_impl: &[TokenStream2],
) -> TokenStream2 {
    let internal_fields_gen = if let Some(row_id_field) = internal_fields {
        quote! {
            #row_id_field: Some(row.get(
                columns.iter().position(|c| c == "rowid")
                    .ok_or_else(|| stash::orm::ConversionError::MissingColumn("rowid".to_owned()))?
            )?),
        }
    } else {
        quote! {}
    };

    let default_fields: Vec<TokenStream2> = all_fields
        .iter()
        .filter(|field| {
            !db_fields.contains(field.ident.as_ref().unwrap())
                && !matches!(
                    internal_fields,
                    Some(row_id_field)
                    if field.ident.as_ref() == Some(row_id_field)
                )
        })
        .map(|field| {
            let field_ident = field.ident.as_ref().unwrap();
            quote! {
                #field_ident: Default::default(),
            }
        })
        .collect();

    quote! {
        fn from_row(row: &stash::exports::Row, columns: &[String]) -> Result<Self, stash::orm::ConversionError> {
            Ok(Self {
                #(
                    #db_fields: #from_row_values_impl,
                )*
                #(#default_fields)*
                #internal_fields_gen
            })
        }
    }
}

/// Generate code implementation for the `id_value()` method.
///
fn generate_fn_id_value_impl(id_field: &Ident, is_optional: bool) -> TokenStream2 {
    if is_optional {
        quote! {
            self.#id_field.clone().ok_or(stash::stash::StashError::IdNotSet)
        }
    } else {
        quote! {
            Ok(self.#id_field.clone())
        }
    }
}

/// Generate code implementation for the `set_id_value()` method.
///
fn generate_fn_set_id_value_impl(id_field: &Ident, is_optional: bool) -> TokenStream2 {
    if is_optional {
        quote! {
            self.#id_field = Some(id);
        }
    } else {
        quote! {
            self.#id_field = id;
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
/// * `via_attrs` - The `via` attributes for the fields. If specified for a
///   field, that field will be converted from a SQL field type to
///   the intermediary type before converting to the struct field
///   type.
///
/// # Panics
///
/// This function panics if any fields do not have an identifier.
///
fn generate_from_row_values_impl(
    db_fields: &[Ident],
    via_attrs: &[Option<ViaIntermediary>],
) -> Vec<TokenStream2> {
    db_fields.iter().zip(via_attrs.iter()).map(|(field_ident, via_attr)| {
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
                ).map_err(|error| {
                    stash::orm::ConversionError::FromSqlConversionError(stringify!(#field_ident).to_owned(), error)
                })?
            }
        }
    })
    .collect()
}
