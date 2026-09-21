use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Ident, ItemFn, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

enum LogArg {
    Simple(Ident),
    Kv {
        key: Ident,
        eq_token: Token![=],
        val: TokenStream2,
    },
}

impl Parse for LogArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Ident) && input.peek2(Token![=]) {
            let key: Ident = input.parse()?;
            let eq_token: Token![=] = input.parse()?;

            let mut val_tokens = TokenStream2::new();
            while !input.is_empty() && !input.peek(Token![,]) {
                let token: proc_macro2::TokenTree = input.parse()?;
                val_tokens.extend(std::iter::once(token));
            }

            Ok(LogArg::Kv {
                key,
                eq_token,
                val: val_tokens,
            })
        } else {
            let ident: Ident = input.parse()?;
            Ok(LogArg::Simple(ident))
        }
    }
}

struct LogArgs {
    fields: Punctuated<LogArg, Token![,]>,
}

impl Parse for LogArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fields = Punctuated::parse_terminated(input)?;
        Ok(LogArgs { fields })
    }
}

#[proc_macro_attribute]
pub fn log(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as LogArgs);
    let input_fn = parse_macro_input!(input as ItemFn);

    let fn_name = input_fn.sig.ident.to_string();

    let fields = args.fields.iter().map(|arg| match arg {
        LogArg::Simple(ident) => {
            quote! {
                #ident = tracing::field::debug(&#ident)
            }
        }
        LogArg::Kv { key, eq_token, val } => {
            quote! {
                #key #eq_token #val
            }
        }
    });

    let expanded = quote! {
        #[tracing::instrument(
            name = #fn_name,
            skip_all,
            fields(#(#fields),*)
        )]
        #input_fn
    };

    expanded.into()
}
