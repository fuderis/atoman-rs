use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Ident, ItemFn, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

enum FieldVal {
    Expr(TokenStream2),
    Display(TokenStream2),
    Debug(TokenStream2),
}

enum LogArg {
    Simple(Ident),
    Kv { key: Ident, val: FieldVal },
}

impl Parse for LogArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Ident) && input.peek2(Token![=]) {
            let key: Ident = input.parse()?;
            let _: Token![=] = input.parse()?;

            let val = if input.peek(Token![%]) {
                let _: Token![%] = input.parse()?;
                FieldVal::Display(parse_val_tokens(input))
            } else if input.peek(Token![?]) {
                let _: Token![?] = input.parse()?;
                FieldVal::Debug(parse_val_tokens(input))
            } else {
                FieldVal::Expr(parse_val_tokens(input))
            };

            Ok(LogArg::Kv { key, val })
        } else {
            let ident: Ident = input.parse()?;
            Ok(LogArg::Simple(ident))
        }
    }
}

fn parse_val_tokens(input: ParseStream) -> TokenStream2 {
    let mut val_tokens = TokenStream2::new();
    while !input.is_empty() && !input.peek(Token![,]) {
        let token: proc_macro2::TokenTree = input.parse().unwrap();
        val_tokens.extend(std::iter::once(token));
    }
    val_tokens
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
    let fn_vis = &input_fn.vis;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;
    let fn_attrs = &input_fn.attrs;

    let fields_formatted = args.fields.iter().map(|arg| match arg {
        LogArg::Simple(ident) => quote! { #ident = ?#ident },
        LogArg::Kv { key, val } => match val {
            FieldVal::Display(expr) => quote! { #key = %(#expr) },
            FieldVal::Debug(expr) => quote! { #key = ?(#expr) },
            FieldVal::Expr(expr) => quote! { #key = (#expr) },
        },
    });

    let is_async = input_fn.sig.asyncness.is_some();

    let body = if is_async {
        quote! {
            let __span = ::atoman::tracing::info_span!(#fn_name, #(#fields_formatted),*);
            use ::atoman::tracing::Instrument;
            async move {
                #fn_block
            }.instrument(__span).await
        }
    } else {
        quote! {
            let __span = ::atoman::tracing::info_span!(#fn_name, #(#fields_formatted),*);
            let __enter = __span.enter();
            #fn_block
        }
    };

    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            #body
        }
    };

    expanded.into()
}
