use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let name = &input.sig.ident;
    let body = &input.block;
    let attrs = &input.attrs;
    let vis = &input.vis;
    let asyncness = &input.sig.asyncness;
    let ret_type = &input.sig.output;

    if asyncness.is_none() {
        return syn::Error::new_spanned(name, "fn must be async")
            .to_compile_error()
            .into();
    }

    let output = quote! {
        #(#attrs)*
        #vis fn #name() #ret_type {
            ::atoman::tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to build Tokio runtime")
                .block_on(async move {
                    #body
                })
        }
    };

    output.into()
}

#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let name = &input.sig.ident;
    let body = &input.block;
    let attrs = &input.attrs;
    let asyncness = &input.sig.asyncness;
    let ret_type = &input.sig.output;

    if asyncness.is_none() {
        return syn::Error::new_spanned(name, "test fn must be async")
            .to_compile_error()
            .into();
    }

    let output = quote! {
        #[test]
        #(#attrs)*
        fn #name() #ret_type {
            ::atoman::tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build Tokio runtime for test")
                .block_on(async move {
                    #body
                })
        }
    };

    output.into()
}
