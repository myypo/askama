use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::crabstar::{CrabstarArgs, complete_ident};

pub(crate) struct PageImplArgs<'a> {
    pub args: &'a CrabstarArgs,
    pub ident: &'a Ident,
    pub generic_params: &'a TokenStream,
    pub generic_args: &'a TokenStream,
    pub where_clause: &'a Option<syn::WhereClause>,
}

pub(crate) fn page_impl<'a>(
    PageImplArgs {
        args,
        ident,
        generic_params,
        generic_args,
        where_clause,
    }: PageImplArgs<'a>,
) -> TokenStream {
    let Some(page) = &args.page else {
        return quote! {};
    };

    let status = match &page.status {
        Some(status) => quote! { ::axum::http::StatusCode::#status },
        None => quote! { ::axum::http::StatusCode::OK },
    };

    // let suspense = !args.suspense.is_empty();
    let suspense = args.suspense.iter().any(|f| f.template.is_some());
    let body = if suspense {
        let stream_impl = quote! {
            struct UnboundedReceiverStream<T>(::tokio::sync::mpsc::UnboundedReceiver<T>);
            impl<T> ::futures::stream::Stream for UnboundedReceiverStream<T> {
                type Item = T;

                fn poll_next(
                        mut self: ::std::pin::Pin<&mut Self>,
                        cx: &mut ::std::task::Context<'_>,
                    ) -> ::std::task::Poll<::std::option::Option<Self::Item>> {
                    self.0.poll_recv(cx)
                }
            }

            UnboundedReceiverStream(rx)
        };

        quote! {
            use ::crabstar::askama::Template;
            use ::axum::response::IntoResponse;

            let body = {
                let (tx, rx) = ::tokio::sync::mpsc::unbounded_channel();
                ::tokio::spawn(async move {
                    use ::crabstar::suspense::Suspense;
                    if let Err(e) = self.suspense(&tx).await {
                        let _ = tx.send(Err(e.into()));
                    }
                });

                #stream_impl
            };
            let body = ::axum::body::Body::from_stream(body);

            match ::axum::response::Response::builder()
                .status(#status)
                .header("Content-Type", "text/html; charset=UTF-8")
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-transform")
                .header("Transfer-Encoding", "chunked")
                .body(body)
            {
                Ok(r) => r,
                Err(err) => {
                    return ::axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
    } else {
        quote! {
            use ::crabstar::askama::Template;
            let mut body = match self.render() {
                Ok(body) => body,
                Err(_) => return ::axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            (#status, ::axum::response::Html(body)).into_response()
        }
    };

    let ident = if suspense {
        &complete_ident(ident)
    } else {
        ident
    };

    let ref_impl = if suspense {
        quote! {}
    } else {
        quote! {
            impl #generic_params ::axum::response::IntoResponse for &#ident #generic_args #where_clause {
                fn into_response(self) -> ::axum::response::Response {
                    #body
                }
            }
        }
    };

    quote! {
        impl #generic_params ::axum::response::IntoResponse for #ident #generic_args #where_clause {
            fn into_response(self) -> ::axum::response::Response {
                #body
            }
        }

        #ref_impl
    }
}
