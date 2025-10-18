#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(elided_lifetimes_in_paths)]
#![deny(unreachable_pub)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "nocrabstar")]
askama_derive::make_derive_template! {
    #[proc_macro_derive(Template, attributes(template, suspense, signal, page))]
    pub fn derive_template() {
        extern crate askama;
    }
}

#[cfg(not(feature = "nocrabstar"))]
askama_derive::make_derive_template! {
    #[proc_macro_derive(Template, attributes(template, suspense, signal, page))]
    pub fn derive_template() {
        use ::crabstar::askama;
    }
}
