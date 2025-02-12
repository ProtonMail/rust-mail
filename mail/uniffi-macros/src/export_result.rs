use crate::prelude::*;
use cruet::Inflector;
use std::rc::Rc;

pub fn expand(mut item: Item) -> TokenStream {
    let mut items = Vec::new();

    visit_mut::visit_item_mut(&mut Visitor::new(&mut items), &mut item);

    quote!(#[::uniffi::export] #item #(#items)*).into()
}

#[derive(Clone, Copy)]
struct FnData<'a> {
    attrs: &'a [Attribute],
    vis: &'a Visibility,
    sig: &'a Signature,
    blk: &'a Block,
}

impl<'a> From<&'a mut ItemFn> for FnData<'a> {
    fn from(i: &'a mut ItemFn) -> Self {
        Self {
            attrs: &i.attrs,
            vis: &i.vis,
            sig: &i.sig,
            blk: &i.block,
        }
    }
}

impl<'a> From<&'a mut ImplItemFn> for FnData<'a> {
    fn from(i: &'a mut ImplItemFn) -> Self {
        Self {
            attrs: &i.attrs,
            vis: &i.vis,
            sig: &i.sig,
            blk: &i.block,
        }
    }
}

struct Visitor<'a> {
    items: &'a mut Vec<Item>,
    stack: Vec<Rc<Type>>,
}

impl<'a> Visitor<'a> {
    fn new(items: &'a mut Vec<Item>) -> Self {
        Self {
            items,
            stack: Vec::new(),
        }
    }

    fn on_visit_fn(&mut self, this: Option<&Type>, data: FnData) -> Option<(ReturnType, Block)> {
        let (t, e) = data.sig.output.get_variants()?;

        let out = self.make_enum(this, &data.sig.ident, &t, &e);
        let exp = self.make_func(this, data);

        let blk = parse_quote!({ #out::from(#exp) });
        let out = parse_quote!(-> #out);

        Some((out, blk))
    }

    fn make_enum(&mut self, this: Option<&Type>, func: &Ident, t: &Type, e: &Type) -> Ident {
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

    fn make_func(&mut self, this: Option<&Type>, data: FnData) -> Expr {
        let sig = data.sig.with_name(format_ident!("__{}", data.sig.ident));

        if let Some(this) = this {
            let attrs = data.attrs;
            let vis = data.vis;
            let blk = data.blk;

            self.push_item(parse_quote!(impl #this { #(#attrs)* #vis #sig #blk }));
        } else {
            let attrs = data.attrs;
            let vis = data.vis;
            let blk = data.blk;

            self.push_item(parse_quote!( #(#attrs)* #vis #sig #blk));
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

impl visit_mut::VisitMut for Visitor<'_> {
    fn visit_item_fn_mut(&mut self, i: &mut ItemFn) {
        visit_mut::visit_item_fn_mut(self, i);

        if let Some((out, blk)) = self.on_visit_fn(None, i.into()) {
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

        if let Some((out, blk)) = self.on_visit_fn(self.self_type().as_deref(), i.into()) {
            i.sig.output = out;
            i.block = blk;
        }
    }
}
