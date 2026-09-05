use heck::ToShoutySnakeCase;
use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemStruct, parse_macro_input};

#[proc_macro_attribute]
pub fn config(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let struct_name = &input.ident;

    // generate name of the static variable: Settings -> SETTINGS_CONFIG (or SETTINGS)
    let static_name_str = format!("{}_CONFIG", struct_name.to_string().to_shouty_snake_case());
    let static_ident = syn::Ident::new(&static_name_str, struct_name.span());

    let expanded = quote! {
        // preserve original structure
        #input

        // static status store
        static #static_ident: atoman::State<atoman::Config<#struct_name>> = atoman::State::default();

        // helper impl for the structure.
        impl #struct_name {
            /// Reads & initializes the config
            pub async fn init<P>(file_path: P) -> ::std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
            where
                P: ::std::convert::AsRef<::std::path::Path>,
            {
                let conf = atoman::Config::<#struct_name>::new(file_path.as_ref()).await?;
                #static_ident.set(conf).await;
                Ok(())
            }

            /// Returns config file path
            pub fn path() -> ::std::path::PathBuf {
                #static_ident.dirty_get().path().clone()
            }

            /// Returns global config instance (zero-copy reference)
            pub fn get() -> ::std::sync::Arc<atoman::Config<#struct_name>> {
                #static_ident.dirty_get()
            }

            /// Returns global config instance (guaranteed to be relevant)
            pub async fn get_actual() -> ::std::sync::Arc<atoman::Config<#struct_name>> {
                #static_ident.get().await
            }

            /// Returns config state guard
            pub async fn lock() -> atoman::StateGuard<'static, atoman::Config<#struct_name>> {
                #static_ident.lock().await
            }

            /// Returns actual config file data
            pub async fn read() -> ::std::result::Result<atoman::Config<#struct_name>, Box<dyn std::error::Error + Send + Sync>> {
                let path = #static_ident.dirty_get().path().clone();
                atoman::Config::<#struct_name>::read(path).await
            }

            /// Updates config from file
            pub async fn update() -> ::std::result::Result<bool, Box<dyn std::error::Error + Send + Sync>> {
                let mut cfg = #static_ident.lock().await;

                if cfg.check(0).await? {
                    cfg.update().await
                } else {
                    Ok(false)
                }
            }
        }
    };

    TokenStream::from(expanded)
}
