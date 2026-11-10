// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! The expansion of `#[derive(Visit)]` and `#[derive(VisitMut)]`.
//!
//! Each enum variant gets its own out-of-line function, so the generated
//! `visit` holds no fields of its own while it recurses. Debug builds give
//! every temporary a stack slot; without this, `Expr`'s frame would hold the
//! slots of all its variants at every level of the tree.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{
    Attribute, Data, DeriveInput, Field, Fields, FieldsNamed, FieldsUnnamed, GenericParam,
    Generics, Ident, Index, LitStr, Path, PathArguments, Token, Type,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Shared,
    Mutable,
}

impl Mode {
    fn visit_trait(self) -> Tokens {
        match self {
            Mode::Shared => quote!(::sqlparser::ast::Visit),
            Mode::Mutable => quote!(::sqlparser::ast::VisitMut),
        }
    }

    fn visitor_trait(self) -> Tokens {
        match self {
            Mode::Shared => quote!(::sqlparser::ast::Visitor),
            Mode::Mutable => quote!(::sqlparser::ast::VisitorMut),
        }
    }

    fn reference(self) -> Tokens {
        match self {
            Mode::Shared => quote!(&),
            Mode::Mutable => quote!(&mut),
        }
    }
}

pub fn derive(input: TokenStream, mode: Mode) -> TokenStream {
    match syn::parse(input).and_then(|input| expand(&input, mode)) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

/// How the generated impl names the lifetime and the AST it belongs to.
struct Names {
    lifetime: Tokens,
    ext: Tokens,
    /// The parameters of the `impl`, without angle brackets.
    impl_params: Tokens,
    ty_generics: Tokens,
}

fn names(input: &DeriveInput, ext: Option<&Path>) -> syn::Result<Names> {
    let generics: &Generics = &input.generics;
    let mut lifetimes = generics.lifetimes();
    let declared = lifetimes.next();
    if lifetimes.next().is_some() {
        return Err(syn::Error::new_spanned(
            generics,
            "Visit supports one lifetime parameter",
        ));
    }
    let type_params: Vec<_> = generics.type_params().collect();
    if type_params.len() > 1 {
        return Err(syn::Error::new_spanned(
            generics,
            "Visit supports one type parameter, which must be the AST extension",
        ));
    }
    if ext.is_some() && !type_params.is_empty() {
        return Err(syn::Error::new_spanned(
            generics,
            "`ext` is for types with no type parameter",
        ));
    }
    let lifetime = match declared {
        Some(param) => {
            let lifetime = &param.lifetime;
            quote!(#lifetime)
        }
        None => quote!('a),
    };
    let ty_args: Vec<Tokens> = generics
        .params
        .iter()
        .map(|param| match param {
            GenericParam::Lifetime(param) => {
                let lifetime = &param.lifetime;
                quote!(#lifetime)
            }
            GenericParam::Type(param) => {
                let ident = &param.ident;
                quote!(#ident)
            }
            GenericParam::Const(param) => {
                let ident = &param.ident;
                quote!(#ident)
            }
        })
        .collect();
    let ty_generics = if ty_args.is_empty() {
        quote!()
    } else {
        quote!(<#(#ty_args),*>)
    };
    let (ext_name, ext_param) = match (ext, type_params.first()) {
        (Some(path), _) => (quote!(#path), None),
        (None, Some(param)) => {
            let ident = &param.ident;
            (
                quote!(#ident),
                Some(quote!(#ident: ::sqlparser::dialect::AstExt)),
            )
        }
        (None, None) => (quote!(X), Some(quote!(X: ::sqlparser::dialect::AstExt))),
    };
    let impl_params = match ext_param {
        Some(param) => quote!(#lifetime, #param),
        None => quote!(#lifetime),
    };
    Ok(Names {
        lifetime,
        ext: ext_name,
        impl_params,
        ty_generics,
    })
}

fn expand(input: &DeriveInput, mode: Mode) -> syn::Result<Tokens> {
    let attrs = Attrs::parse(&input.attrs)?;
    let names = names(input, attrs.ext.as_ref())?;
    let name = &input.ident;
    let Names {
        lifetime,
        ext,
        impl_params,
        ty_generics,
    } = &names;
    let visit_trait = mode.visit_trait();
    let visitor_trait = mode.visitor_trait();
    let reference = mode.reference();
    let self_ty = quote!(#name #ty_generics);

    let ctx = Context {
        mode,
        names: &names,
        name,
    };
    let body = match &input.data {
        Data::Struct(data) => struct_body(&ctx, &data.fields, &attrs)?,
        Data::Enum(data) => enum_body(&ctx, &data.variants, &attrs)?,
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                input,
                "Visit does not support unions",
            ));
        }
    };
    Ok(quote! {
        #[automatically_derived]
        impl<#impl_params> #visit_trait<#lifetime, #ext> for #self_ty {
            fn visit<__V: #visitor_trait<#lifetime, #ext>>(
                #reference self,
                visitor: &mut __V,
            ) -> ::core::ops::ControlFlow<__V::Break> {
                #body
            }
        }
    })
}

struct Context<'t> {
    mode: Mode,
    names: &'t Names,
    name: &'t Ident,
}

/// `#[visit(...)]`, as parsed from a type or a field.
#[derive(Default)]
struct Attrs {
    with: Option<Ident>,
    when: Option<Path>,
    whole: Option<Ident>,
    nonempty: Option<Ident>,
    skip_if: Option<Path>,
    ext: Option<Path>,
}

impl Attrs {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut out = Self::default();
        for attr in attrs {
            if !attr.path().is_ident("visit") {
                continue;
            }
            attr.parse_args_with(|input: ParseStream| {
                while !input.is_empty() {
                    let key: Ident = input.parse()?;
                    input.parse::<Token![=]>()?;
                    let value: LitStr = input.parse()?;
                    match key.to_string().as_str() {
                        "with" => out.with = Some(value.parse()?),
                        "when" => out.when = Some(value.parse()?),
                        "whole" => out.whole = Some(value.parse()?),
                        "nonempty" => out.nonempty = Some(value.parse()?),
                        "skip_if" => out.skip_if = Some(value.parse()?),
                        "ext" => out.ext = Some(value.parse()?),
                        other => {
                            return Err(syn::Error::new(
                                key.span(),
                                format!("unknown visit option `{other}`"),
                            ));
                        }
                    }
                    if input.peek(Token![,]) {
                        input.parse::<Token![,]>()?;
                    }
                }
                Ok(())
            })?;
        }
        Ok(out)
    }

    /// The pre and post calls for a hook, around `value`.
    fn hooks(hook: &Ident, value: &Tokens) -> (Tokens, Tokens) {
        let pre = format_ident!("pre_{hook}");
        let post = format_ident!("post_{hook}");
        (
            quote!(visitor.#pre(#value)?;),
            quote!(visitor.#post(#value)?;),
        )
    }
}

fn visit_call(ctx: &Context<'_>, value: &Tokens) -> Tokens {
    let visit_trait = ctx.mode.visit_trait();
    let Names { lifetime, ext, .. } = ctx.names;
    quote!(<_ as #visit_trait<#lifetime, #ext>>::visit(#value, visitor)?;)
}

fn single_argument(args: &PathArguments) -> Option<&Type> {
    let PathArguments::AngleBracketed(args) = args else {
        return None;
    };
    match args.args.first() {
        Some(syn::GenericArgument::Type(ty)) => Some(ty),
        _ => None,
    }
}

/// Peels `Option`, `Vec`, `Box` and tuples off a field, calling the hooks of
/// `with` around each value inside.
fn hooked_field(ctx: &Context<'_>, ty: &Type, value: Tokens, hook: &Ident) -> Tokens {
    if let Type::Tuple(tuple) = ty {
        let parts = tuple.elems.iter().enumerate().map(|(i, element)| {
            let index = Index::from(i);
            let reference = ctx.mode.reference();
            let part = quote!(#reference (*#value).#index);
            if is_text(element) {
                visit_call(ctx, &part)
            } else {
                hooked_field(ctx, element, part, hook)
            }
        });
        return quote!(#(#parts)*);
    }
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
        && let Some(inner) = single_argument(&segment.arguments)
    {
        let name = segment.ident.to_string();
        match name.as_str() {
            "Option" => {
                let item = quote!(__item);
                let inner = hooked_field(ctx, inner, item.clone(), hook);
                return quote!(if let ::core::option::Option::Some(#item) = #value { #inner });
            }
            "Vec" => {
                let item = quote!(__item);
                let inner = hooked_field(ctx, inner, item.clone(), hook);
                return quote!(for #item in #value { #inner });
            }
            "Box" => {
                let reference = ctx.mode.reference();
                return hooked_field(ctx, inner, quote!(#reference **#value), hook);
            }
            _ => {}
        }
    }
    let (pre, post) = Attrs::hooks(hook, &value);
    let visit = visit_call(ctx, &value);
    quote!(#pre #visit #post)
}

fn field_tokens(
    ctx: &Context<'_>,
    field: &Field,
    value: Tokens,
    this: &Tokens,
    in_struct: bool,
) -> syn::Result<Tokens> {
    let attrs = Attrs::parse(&field.attrs)?;
    if (attrs.when.is_some() || attrs.skip_if.is_some()) && !in_struct {
        return Err(syn::Error::new(
            field.span(),
            "`when` and `skip_if` are only for struct fields",
        ));
    }
    let shared = quote!(&*#this);
    let plain = visit_call(ctx, &value);
    let hooked = if let Some(hook) = &attrs.with {
        let hooked = hooked_field(ctx, &field.ty, value.clone(), hook);
        match &attrs.when {
            Some(predicate) => quote!(if #predicate(#shared) { #hooked } else { #plain }),
            None => hooked,
        }
    } else if let Some(hook) = &attrs.whole {
        let (pre, post) = Attrs::hooks(hook, &value);
        quote!(#pre #plain #post)
    } else if let Some(hook) = &attrs.nonempty {
        let (pre, post) = Attrs::hooks(hook, &value);
        quote!(if !(#value).is_empty() { #pre #plain #post })
    } else {
        plain
    };
    Ok(match &attrs.skip_if {
        Some(predicate) => quote!(if !#predicate(#shared) { #hooked }),
        None => hooked,
    })
}

fn struct_body(ctx: &Context<'_>, fields: &Fields, attrs: &Attrs) -> syn::Result<Tokens> {
    let this = quote!(self);
    let reference = ctx.mode.reference();
    let visits: Vec<Tokens> = match fields {
        Fields::Named(FieldsNamed { named, .. }) => named
            .iter()
            .map(|field| {
                let member = field.ident.as_ref().expect("named field");
                field_tokens(ctx, field, quote!(#reference #this.#member), &this, true)
            })
            .collect::<syn::Result<_>>()?,
        Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => unnamed
            .iter()
            .enumerate()
            .map(|(i, field)| {
                let index = Index::from(i);
                field_tokens(ctx, field, quote!(#reference #this.#index), &this, true)
            })
            .collect::<syn::Result<_>>()?,
        Fields::Unit => Vec::new(),
    };
    Ok(wrap_with_hooks(attrs, quote!(#(#visits)*), quote!(self)))
}

fn wrap_with_hooks(attrs: &Attrs, children: Tokens, this: Tokens) -> Tokens {
    match &attrs.with {
        Some(hook) => {
            let (pre, post) = Attrs::hooks(hook, &this);
            quote! {
                #pre
                #children
                #post
                ::core::ops::ControlFlow::Continue(())
            }
        }
        None => quote! {
            #children
            ::core::ops::ControlFlow::Continue(())
        },
    }
}

fn enum_body(
    ctx: &Context<'_>,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
    attrs: &Attrs,
) -> syn::Result<Tokens> {
    let name = ctx.name;
    let mut functions = Vec::new();
    let mut arms = Vec::new();
    for (index, variant) in variants.iter().enumerate() {
        let vident = &variant.ident;
        let pattern_empty = match &variant.fields {
            Fields::Named(_) => quote!({ .. }),
            Fields::Unnamed(_) => quote!((..)),
            Fields::Unit => quote!(),
        };
        if let Fields::Unit = variant.fields {
            arms.push(quote!(#name::#vident => ::core::ops::ControlFlow::Continue(()),));
            continue;
        }
        let function = format_ident!("__visit_variant_{index}", span = Span::call_site());
        functions.push(variant_function(ctx, &function, variant)?);
        arms.push(quote!(#name::#vident #pattern_empty => #function(self, visitor),));
    }
    let matched = quote!(match self { #(#arms)* });
    let body = match &attrs.with {
        Some(hook) => {
            let (pre, post) = Attrs::hooks(hook, &quote!(self));
            quote! {
                #pre
                (#matched)?;
                #post
                ::core::ops::ControlFlow::Continue(())
            }
        }
        None => matched,
    };
    Ok(quote! {
        #(#functions)*
        #body
    })
}

/// One variant's out-of-line function. It takes the whole node and matches it
/// again, to bind the fields only inside its own frame.
fn variant_function(
    ctx: &Context<'_>,
    function: &Ident,
    variant: &syn::Variant,
) -> syn::Result<Tokens> {
    let name = ctx.name;
    let vident = &variant.ident;
    let Names {
        lifetime,
        ext,
        impl_params,
        ty_generics,
    } = ctx.names;
    let reference = ctx.mode.reference();
    let visitor_trait = ctx.mode.visitor_trait();
    let (pattern, visits): (Tokens, Vec<Tokens>) = match &variant.fields {
        Fields::Named(fields) => {
            let names: Vec<_> = fields
                .named
                .iter()
                .map(|field| field.ident.clone().expect("named field"))
                .collect();
            let visits = fields
                .named
                .iter()
                .zip(&names)
                .map(|(field, member)| {
                    field_tokens(ctx, field, quote!(#member), &quote!(this), false)
                })
                .collect::<syn::Result<Vec<_>>>()?;
            (quote!(#name::#vident { #(#names),* }), visits)
        }
        Fields::Unnamed(fields) => {
            let names: Vec<_> = (0..fields.unnamed.len())
                .map(|i| format_ident!("__field_{i}"))
                .collect();
            let visits = fields
                .unnamed
                .iter()
                .zip(&names)
                .map(|(field, member)| {
                    field_tokens(ctx, field, quote!(#member), &quote!(this), false)
                })
                .collect::<syn::Result<Vec<_>>>()?;
            (quote!(#name::#vident ( #(#names),* )), visits)
        }
        Fields::Unit => unreachable!("unit variants have no function"),
    };
    Ok(quote! {
        #[inline(never)]
        #[allow(clippy::needless_return)]
        fn #function<#impl_params, __V: #visitor_trait<#lifetime, #ext>>(
            this: #reference #name #ty_generics,
            visitor: &mut __V,
        ) -> ::core::ops::ControlFlow<__V::Break> {
            match this {
                #pattern => { #(#visits)* }
                #[allow(unreachable_patterns)]
                _ => {}
            }
            ::core::ops::ControlFlow::Continue(())
        }
    })
}

/// True for a string inside a tuple, which is never the hooked value.
fn is_text(ty: &Type) -> bool {
    match ty {
        Type::Reference(_) => true,
        Type::Path(path) => path.path.segments.last().is_some_and(|s| s.ident == "Cow"),
        _ => false,
    }
}
