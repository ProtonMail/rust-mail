use crate::internal::prelude::*;
use syn::punctuated::Punctuated;

#[must_use]
pub fn type_iter<T: Into<TokenStream>>(item: T) -> TokenStream {
    let item = item.into();

    render!(item, { expand(&item) })
}

fn expand(input: &DeriveInput) -> Result<TokenStream> {
    match &input.data {
        Data::Struct(data) => expand_struct(&input.ident, data, &input.generics),
        Data::Enum(data) => expand_enum(&input.ident, data, &input.generics),
        Data::Union(_) => Err(Error::new_spanned(input, "unsupported")),
    }
}

fn expand_struct(ident: &Ident, data: &DataStruct, generics: &Generics) -> Result<TokenStream> {
    let iter = expand_fields(&parse_quote!(Self), &data.fields)?;

    let (igen, tgen, wgen) = generics.split_for_impl();

    Ok(quote! {
        const _: () = {
            impl #igen ::muon::common::TypeIter for #ident #tgen #wgen {
                fn iter() -> impl ::std::iter::Iterator<Item = Self> + Clone {
                    #iter
                }
            }
        };
    })
}

fn expand_enum(ident: &Ident, data: &DataEnum, generics: &Generics) -> Result<TokenStream> {
    let mut iter = Vec::new();

    for Variant { ident, fields, .. } in &data.variants {
        iter.push(expand_fields(&parse_quote!(Self::#ident), fields)?);
    }

    let (igen, tgen, wgen) = generics.split_for_impl();

    Ok(quote! {
        const _: () = {
            impl #igen ::muon::common::TypeIter for #ident #tgen #wgen {
                fn iter() -> impl ::std::iter::Iterator<Item = Self> + Clone {
                    ::muon::deps::itertools::chain!(#(#iter,)*)
                }
            }
        };
    })
}

fn expand_fields(this: &Path, fields: &Fields) -> Result<TokenStream> {
    match fields {
        Fields::Named(fields) => expand_fields_named(this, fields),
        Fields::Unnamed(fields) => expand_fields_unnamed(this, fields),
        Fields::Unit => Ok(quote!(::std::iter::once(#this))),
    }
}

fn expand_fields_named(this: &Path, fields: &FieldsNamed) -> Result<TokenStream> {
    let mut names = Vec::new();
    let mut iters = Vec::new();

    for field in &fields.named {
        names.push(&field.ident);
        iters.push(expand_field(field)?);
    }

    Ok(quote! {
        ::muon::deps::itertools::iproduct!(#(#iters,)*)
            .map(|(#(#names,)*)| #this { #(#names,)* })
    })
}

/// Produces an iterator for the unnamed fields of a struct or enum.
fn expand_fields_unnamed(this: &Path, fields: &FieldsUnnamed) -> Result<TokenStream> {
    let mut names = Vec::new();
    let mut iters = Vec::new();

    for (idx, field) in fields.unnamed.iter().enumerate() {
        names.push(format_ident!("f{idx}"));
        iters.push(expand_field(field)?);
    }

    Ok(quote! {
        ::muon::deps::itertools::iproduct!(#(#iters,)*)
            .map(|(#(#names,)*)| #this(#(#names,)*))
    })
}

/// Produces an iterator for the inner type of a field.
fn expand_field(field: &Field) -> Result<TokenStream> {
    if let Some(attrs) = FieldAttrs::maybe_new(&field.attrs)? {
        expand_field_attrs(field, attrs)
    } else {
        Ok(expand_field_type(&field.ty))
    }
}

fn expand_field_attrs(field: &Field, attrs: FieldAttrs) -> Result<TokenStream> {
    if let Some(values) = attrs.values {
        Ok(expand_field_values(&values))
    } else {
        expand_field_range(field, attrs.beg, attrs.end, attrs.add, attrs.mul)
    }
}

fn expand_field_range(
    field: &Field,
    beg: Option<Expr>,
    end: Option<Expr>,
    add: Option<Expr>,
    mul: Option<Expr>,
) -> Result<TokenStream> {
    let beg = beg.ok_or(Error::new_spanned(field, "missing begin"))?;
    let end = end.ok_or(Error::new_spanned(field, "missing end"))?;

    let (add, mul) = match (add, mul) {
        (None, None) => (quote!(1), quote!(1)),
        (None, Some(mul)) => (quote!(0), quote!(#mul)),
        (Some(add), None) => (quote!(#add), quote!(1)),
        (Some(add), Some(mul)) => (quote!(#add), quote!(#mul)),
    };

    Ok(quote!(::muon::common::RangeIter::new(#beg, #end, #add, #mul)))
}

fn expand_field_values(values: &ExprArray) -> TokenStream {
    quote!(#values)
}

fn expand_field_type(ty: &Type) -> TokenStream {
    quote!(<#ty as ::muon::common::TypeIter>::iter())
}

/// The comma-separated attributes of a field.
type FieldMeta = Punctuated<Meta, Token![,]>;

/// The attributes of a field.
#[derive(Default)]
struct FieldAttrs {
    // Explicit values:
    values: Option<ExprArray>,

    // Range values:
    beg: Option<Expr>,
    end: Option<Expr>,
    add: Option<Expr>,
    mul: Option<Expr>,
}

impl FieldAttrs {
    fn maybe_new(attrs: &[Attribute]) -> Result<Option<Self>> {
        let mut res = None;

        for attr in attrs {
            if attr.path().is_ident("iter") {
                let this = res.get_or_insert_with(Self::default);

                for meta in attr.parse_args_with(FieldMeta::parse_terminated)? {
                    let Meta::List(meta) = meta else {
                        return Err(Error::new_spanned(meta, "invalid meta"));
                    };

                    if meta.path.is_ident("values") {
                        this.values = Some(meta.parse_args()?);
                    }

                    if meta.path.is_ident("begin") {
                        this.beg = Some(meta.parse_args()?);
                    }

                    if meta.path.is_ident("end") {
                        this.end = Some(meta.parse_args()?);
                    }

                    if meta.path.is_ident("add") {
                        this.add = Some(meta.parse_args()?);
                    }

                    if meta.path.is_ident("mul") {
                        this.mul = Some(meta.parse_args()?);
                    }
                }
            }
        }

        Ok(res)
    }
}
