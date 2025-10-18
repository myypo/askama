use std::sync::Arc;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::spanned::Spanned;
use syn::{Ident, LitStr, Visibility};

use crate::crabstar::args::CrabstarSuspenseArgs;
use crate::crabstar::{CrabstarArgs, complete_ident};

fn suspense_body<'a>(
    page: bool,
    path: &'a LitStr,
    suspense_fields: &'a [SuspenseField],
) -> TokenStream {
    let immediate = if suspense_fields.is_empty() {
        quote! { self }
    } else {
        quote! { self.0 }
    };

    let immediate_call = if page {
        quote! {
            use ::crabstar::askama::Template;
            let mut r = #immediate.render().map_err(::std::convert::Into::into);
            tx.send(r)
        }
    } else {
        // TODO: there might be a way to do it at compile-time
        // like creating two templates at the same time
        // one for child suspense use and the other for PatchElements etc.
        // or just figure out some mono-attribute macro approach
        quote! {
            use ::crabstar::askama::Template;
            let mut r = format!(r#"<template id="crabstar-template-{}" data-on-load="streamSsr(el.id, '{}')">"#, #path, #path);
            let result = #immediate.render_into(&mut r).map_err(::std::convert::Into::into);
            let mut r = result.map(|_| r);
            if let Ok(r) = &mut r {
                r.push_str("</template>");
            }
            tx.send(r)
        }
    };

    if suspense_fields.is_empty() {
        quote! { #immediate_call }
    } else {
        let calls = suspense_fields.iter().map(|f| {
            let field_ident = &f.ident;

            quote! {
                let #field_ident = self.1.#field_ident;
                let #field_ident = #field_ident.then(|n| n.suspense(&tx)).boxed();
            }
        });

        let suspense_field_idents = suspense_fields.iter().map(|f| &f.ident);

        quote! {
            #immediate_call?;

            use ::crabstar::suspense::Suspense;
            #(#calls)*

            ::futures::future::join_all(
                [#(#suspense_field_idents),*]
            ).await;

            Ok(())
        }
    }
}

pub(crate) struct SuspenseImplArgs<'a> {
    pub path: &'a Arc<str>,
    pub args: &'a CrabstarArgs,

    pub ident: &'a Ident,
    pub vis: &'a Visibility,
    pub generic_params: &'a TokenStream,
    pub generic_args: &'a TokenStream,
}

struct SuspenseField {
    ident: Ident,
    future: Ident,
    output: TokenStream,
}

fn to_snake_case(ident: &Ident) -> Ident {
    let s = ident.to_string();
    let mut result = String::new();

    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        result.push(first.to_ascii_lowercase());
    }
    for c in chars {
        if c.is_uppercase() {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }

    Ident::new(&result, ident.span())
}

fn suspense_fields_from_args(args: &[CrabstarSuspenseArgs]) -> Vec<SuspenseField> {
    args.iter()
        .filter_map(|f| f.template.as_ref().map(|t| (t, &f.name)))
        .enumerate()
        .map(|(i, (t, name))| {
            let span = t.span();

            let ident = name.clone().unwrap_or_else(|| {
                t.segments
                    .last()
                    .map(|seg| to_snake_case(&seg.ident))
                    .unwrap()
            });
            let future = Ident::new(&format!("F{i}"), span);

            let full_path = t
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<String>>()
                .join("::");
            let output = complete_ident(&Ident::new(&full_path, span));
            let output = quote! { ::std::result::Result<#output, ::crabstar::suspense::Error> };

            SuspenseField {
                ident,

                future,
                output,
            }
        })
        .collect()
}

pub(crate) fn suspense_impl<'a>(
    SuspenseImplArgs {
        path,
        args,
        ident,
        vis,
        generic_params,
        generic_args,
    }: SuspenseImplArgs<'a>,
) -> TokenStream {
    let suspense_fields = &args.suspense;
    if suspense_fields.is_empty() {
        return quote! {};
    }
    let suspense_fields = suspense_fields_from_args(&args.suspense);

    let complete_ident = complete_ident(ident);
    let suspense_ident = Ident::new(&format!("{ident}Suspense"), ident.span());
    let boxed_suspense_ident = Ident::new(&format!("{ident}BoxedSuspense"), ident.span());
    let path = LitStr::new(path, Span::call_site());

    let where_clause = if suspense_fields.is_empty() {
        quote! {}
    } else {
        let where_params = suspense_fields.iter().map(
            |SuspenseField { output, future, .. }| {
                quote! {
                    #future: ::std::future::Future<Output = #output> + ::std::marker::Send + 'static
                }
            },
        );

        quote! {
            where
                #(#where_params,)*
        }
    };

    let future_generic_params: Vec<TokenStream> = suspense_fields
        .iter()
        .map(|SuspenseField { future, .. }| {
            quote! { #future }
        })
        .collect();

    let suspense_struct = if suspense_fields.is_empty() {
        quote! {}
    } else {
        let suspense_fields = suspense_fields.iter().map(
            |SuspenseField {
                 ident: field_ident,
                 future,
                 ..
             }| {
                quote! {
                    #vis #field_ident: #future
                }
            },
        );

        quote! {
            #vis struct #suspense_ident<#(#future_generic_params,)*>
                #where_clause
            {
                #(#suspense_fields,)*
            }
        }
    };

    let boxed_suspense_struct = if suspense_fields.is_empty() {
        quote! {}
    } else {
        let boxed_suspense_fields = suspense_fields
            .iter()
            .map(|SuspenseField { ident: field_ident, output, .. }| {
                quote! {
                    #vis #field_ident: ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = #output> + ::std::marker::Send + 'static>>
                }
            });

        let suspense_field_idents = suspense_fields.iter().map(|f| &f.ident);

        quote! {
            #vis struct #boxed_suspense_ident {
                #(#boxed_suspense_fields,)*
            }

            impl<#(#future_generic_params,)*> ::std::convert::From<#suspense_ident<#(#future_generic_params,)*>> for #boxed_suspense_ident
                #where_clause
            {
                fn from(value: #suspense_ident<#(#future_generic_params,)*>) -> Self {
                    Self {
                        #(
                            #suspense_field_idents: ::std::boxed::Box::pin(value.#suspense_field_idents),
                        )*
                    }
                }
            }
        }
    };

    let complete_struct = if suspense_fields.is_empty() {
        quote! {
            #[allow(type_alias_bounds)]
            #vis type #complete_ident #generic_params = #ident #generic_args;
        }
    } else {
        quote! {
            #vis struct #complete_ident #generic_params (#ident #generic_args, #boxed_suspense_ident);
        }
    };

    let suspense_body = suspense_body(args.page.is_some(), &path, &suspense_fields);

    let into_suspense_impl = if suspense_fields.is_empty() {
        quote! {}
    } else {
        quote! {
            impl #generic_params #ident #generic_args {
                #vis fn into_suspense<#(#future_generic_params,)*>(self, suspense: #suspense_ident<#(#future_generic_params,)*>) -> #complete_ident #generic_args
                #where_clause
                {
                    #complete_ident(self, suspense.into())
                }
            }
        }
    };

    let boxed_error = quote! { ::std::boxed::Box<dyn ::std::error::Error + ::std::marker::Send + ::std::marker::Sync> };

    quote! {
        #suspense_struct

        #boxed_suspense_struct

        #complete_struct

        impl #generic_params ::crabstar::suspense::Suspense for #complete_ident #generic_args
        {
            async fn suspense(self, tx: &::tokio::sync::mpsc::UnboundedSender<::std::result::Result<::std::string::String, #boxed_error>>)
                -> ::std::result::Result<
                (),
                ::tokio::sync::mpsc::error::SendError<
                    ::std::result::Result<::std::string::String, #boxed_error>>
                >
            {
                use ::futures::FutureExt;

                #suspense_body
            }

            const PATH: &'static str = #path;
        }

        #into_suspense_impl
    }
}
