use crate::prelude::*;
use cruet::Inflector;
use proc_macro::TokenStream;
use std::rc::Rc;

/// Prelude for the crate.
mod prelude;

/// Helper traits for working with `syn` types.
mod traits;

/// This `proc_macro` rewrites the function which return Result<T, E> to return non genric enum type.
/// If used on a function that does not return Result, it will return the original function.
///
/// This is usefull for `#[uniffi::export]` to not throw panics on client side.
/// This macro can be used interchangable with `#[uniffi::export]` attribute as it expands to include it.
///
/// New enum type uses function name as a prefix and `Result` as a suffix.
/// It currently supports only functions and impl blocks. For impl blocks it will rewrite all the methods returning Result type.
///
/// ### Example
///
/// ```ignore
/// #[proton_uniffi_macros::export_result]
/// fn contact_list() -> Result<Vec<Contact>, ContactError> {
///     Ok(vec![])
/// }
/// ```
///
/// will be rewritten as
///
/// ```ignore
/// enum ContactListResult {
///     Ok(Vec<Contact>),
///     Error(ContactError),
/// }
///
/// #[uniffi::export]
/// fn contact_list() -> ContactListResult {
///     ContactListResult::from(__contact_list())
/// }
///
/// fn __contact_list() -> Result<Vec<Contact>, ContactError> {
///     Ok(vec![])
/// }
/// ```
///
/// ### Panics
///
/// This macro will panic if the `TokenStream` is not a function or impl block.
/// Also if the function returns a Result type, it need to have exactly two generic arguments.
///
/// ### Limitations
///
/// Return type must be exactly `Result<T, E>` where `T` and `E` are generic types.
/// There is no support for type aliases or other generic types.
///
/// Take into considaration that if there is a function with the same name as
/// the method in impl block and both are using this macro, there will be a compile error
/// as the enum type will be duplicated.
///
/// The same applies for the `*` imports & exports - this may be addressed in the future.
/// But for now use unique and descripitive names for exported functions and methods.
///
#[proc_macro_attribute]
pub fn export_result(_: TokenStream, input: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(input as Item);
    let mut items = Vec::new();

    visit_mut::visit_item_mut(&mut Visitor::new(&mut items), &mut input);

    quote!(#[::uniffi::export] #input #(#items)*).into()
}

struct Visitor<'a> {
    items: &'a mut Vec<Item>,
    stack: Vec<Rc<Type>>,
}

impl visit_mut::VisitMut for Visitor<'_> {
    fn visit_item_fn_mut(&mut self, i: &mut ItemFn) {
        visit_mut::visit_item_fn_mut(self, i);

        if let Some((out, blk)) = self.on_visit_fn(None, &i.sig, &i.block) {
            i.sig.output = out;
            i.block = Box::new(blk);
        }
    }

    fn visit_item_impl_mut(&mut self, i: &mut ItemImpl) {
        self.push_self(&i.self_ty);

        visit_mut::visit_item_impl_mut(self, i);

        self.pop_self();
    }

    fn visit_impl_item_fn_mut(&mut self, i: &mut ImplItemFn) {
        visit_mut::visit_impl_item_fn_mut(self, i);

        if let Some((out, blk)) = self.on_visit_fn(self.self_type().as_ref(), &i.sig, &i.block) {
            i.sig.output = out;
            i.block = blk;
        }
    }
}

impl<'a> Visitor<'a> {
    fn new(items: &'a mut Vec<Item>) -> Self {
        Self {
            items,
            stack: Vec::new(),
        }
    }

    fn on_visit_fn(
        &mut self,
        this: Option<&Rc<Type>>,
        sig: &Signature,
        blk: &Block,
    ) -> Option<(ReturnType, Block)> {
        let (t, e) = sig.output.get_variants()?;

        let out = self.make_enum(this, &sig.ident, &t, &e);
        let exp = self.make_func(this, sig, blk);

        let blk = parse_quote!({ #out::from(#exp) });
        let out = parse_quote!(-> #out);

        Some((out, blk))
    }

    fn make_enum(&mut self, this: Option<&Rc<Type>>, func: &Ident, t: &Type, e: &Type) -> Ident {
        let name = if let Some(this) = this.as_ident() {
            format_ident!("{}", format!("{this}_{func}_result").to_pascal_case())
        } else {
            format_ident!("{}", format!("{func}_result").to_pascal_case())
        };

        let t_fields = t.as_fields();
        let e_fields = e.as_fields();

        self.push_item(parse_quote! {
            #[automatically_derived]
            #[derive(::uniffi::Enum)]
            pub enum #name {
                Ok #t_fields,
                Error #e_fields,
            }
        });

        let value = format_ident!("value");
        let t_rhs = t.as_match_rhs(&value);
        let e_rhs = e.as_match_rhs(&value);

        self.push_item(parse_quote! {
            #[automatically_derived]
            impl From<::std::result::Result<#t, #e>> for #name {
                fn from(value: ::std::result::Result<#t, #e>) -> Self {
                    match value {
                        Ok(#value) => #name::Ok #t_rhs,
                        Err(#value) => #name::Error #e_rhs,
                    }
                }
            }
        });

        name
    }

    fn make_func(&mut self, this: Option<&Rc<Type>>, sig: &Signature, blk: &Block) -> Expr {
        let sig = sig.with_name(format_ident!("__{}", sig.ident));

        if let Some(this) = this {
            self.push_item(parse_quote!(impl #this { #sig #blk }));
        } else {
            self.push_item(parse_quote!(#sig #blk));
        }

        sig.as_call()
    }

    fn self_type(&self) -> Option<Rc<Type>> {
        self.stack.last().cloned()
    }

    fn push_item(&mut self, item: Item) {
        self.items.push(item);
    }

    fn push_self(&mut self, item: &Type) {
        self.stack.push(Rc::new(item.to_owned()));
    }

    fn pop_self(&mut self) {
        self.stack.pop();
    }
}
