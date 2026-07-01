use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{DeriveInput, Expr, ExprLit, Fields, ItemStruct, Lit, Token};

#[proc_macro_derive(TauriEvent)]
pub fn derive_tauri_event(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    let ident = &input.ident;
    let name = ident.to_string();

    quote! {
        impl crate::api::events::TauriEvent for #ident {
            fn name() -> String {
                #name.to_string()
            }
        }
    }
    .into()
}

#[proc_macro_attribute]
pub fn matrix_settings(_attr: TokenStream, item: TokenStream) -> TokenStream {
    assert!(_attr.is_empty());
    let item_ast = syn::parse(item).unwrap();

    convert_settings(item_ast)
}

fn convert_settings(mut item: ItemStruct) -> TokenStream {
    let struct_name = item.ident.clone();
    let mut default_field_initializers = vec![];
    let mut type_name_string_collector = vec![];

    if let Fields::Named(ref mut fields) = item.fields {
        for field in &mut fields.named {
            let original_type = field.ty.clone();
            let field_name = field.ident.as_ref().unwrap().clone();

            let mut setting_meta = None;

            field.attrs.retain(|attr| {
                if attr.path().is_ident("setting") {
                    if let Ok(exprs) =
                        attr.parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated)
                    {
                        let mut iter = exprs.iter();

                        let human_readable = match iter.next() {
                            Some(Expr::Lit(ExprLit {
                                lit: Lit::Str(lit_str),
                                ..
                            })) => lit_str.value(),
                            _ => panic!("Must be a string literal"),
                        };

                        let uses_cloud = match iter.next() {
                            Some(Expr::Lit(ExprLit {
                                lit: Lit::Bool(lit_bool),
                                ..
                            })) => lit_bool.value,
                            _ => false,
                        };

                        let mut default_expr = quote! { Default::default() };
                        if let Some(Expr::Assign(expr_assign)) = iter.next()
                            && let Expr::Path(ref expr_path) = *expr_assign.left
                            && expr_path.path.is_ident("default")
                        {
                            let right = &expr_assign.right;
                            default_expr = quote! { #right };
                        }

                        setting_meta = Some((human_readable, uses_cloud, default_expr));
                    }
                    false
                } else {
                    true
                }
            });

            if let Some((human_readable, uses_cloud, default_expr)) = setting_meta {
                let type_name_str = format!("com.opal.{}", field_name);

                field.ty = syn::parse2(quote! { MatrixSettingField<#original_type> }).unwrap();

                default_field_initializers.push(quote! {
                    #field_name : MatrixSettingField {
                        val: ::leptos::prelude::RwSignal::new(#default_expr),
                        type_name: #type_name_str,
                        human_readable: #human_readable,
                        uses_cloud: #uses_cloud
                    }
                });
                type_name_string_collector.push((
                    type_name_str.clone(),
                    field_name,
                    original_type,
                    uses_cloud,
                ));
            } else {
                default_field_initializers.push(quote! {
                    #field_name: Default::default()
                });
            }
        }
    } else {
        panic!("Only applicable to structs with named fields")
    }

    let signal_bindings = type_name_string_collector
        .iter()
        .map(|(_, field_name, _, _)| {
            quote! {
                let #field_name = self.#field_name.val;
            }
        });

    // File event: update signal; for cloud fields also push the new value up.
    let match_arms_file: Vec<_> = type_name_string_collector
        .iter()
        .map(|(type_name, field_name, _, uses_cloud)| {
            let upload = if *uses_cloud {
                quote! {
                    let key_c = new.key.clone();
                    let val_c = new.value.clone();
                    ::leptos::task::spawn_local(async move {
                        let args = ::serde_wasm_bindgen::to_value(
                            &::serde_json::json!({ "key": key_c, "value": val_c })
                        ).expect("Failed to serialize cloud-upload args");
                        if let Err(e) = call_tauri("set_setting_cloud", args).await {
                            ::log::error!("Failed to upload '{}' to cloud: {:?}", key_c, e);
                        }
                    });
                }
            } else {
                quote! {}
            };
            quote! {
                #type_name => {
                    match ::serde_json::from_str(&new.value) {
                        Ok(parsed) => #field_name.set(parsed),
                        Err(e) => ::log::warn!("Failed to deserialize field '{}': {:?}", stringify!(#field_name), e)
                    }
                    #upload
                }
            }
        })
        .collect();

    // Cloud event: just update the signal (value already came from the cloud).
    let match_arms_cloud: Vec<_> = type_name_string_collector
        .iter()
        .map(|(type_name, field_name, _, _)| {
            quote! {
                #type_name => match ::serde_json::from_str(&new.value) {
                    Ok(parsed) => #field_name.set(parsed),
                    Err(e) => ::log::warn!("Failed to deserialize field '{}': {:?}", stringify!(#field_name), e)
                }
            }
        })
        .collect();

    let get_all_calls = type_name_string_collector
        .iter()
        .map(|(_, field_name, _, _)| {
            let get_fn = format_ident!("get_{}", field_name);
            quote! { self.#get_fn().await?; }
        });

    let accessor_methods = type_name_string_collector
        .iter()
        .map(|(type_name, field_name, original_type, uses_cloud)| {
            let get_fn = format_ident!("get_{}", field_name);
            let set_fn = format_ident!("set_{}", field_name);
            quote! {
                pub fn #set_fn(&self, val: #original_type) {
                    let serialized = match ::serde_json::to_string(&val) {
                        Ok(s) => s,
                        Err(e) => { ::log::error!("Failed to serialize {}: {:?}", stringify!(#field_name), e); return; }
                    };
                    self.#field_name.val.set(val);
                    let args = ::serde_wasm_bindgen::to_value(
                        &::serde_json::json!({ "key": #type_name, "value": serialized, "to_cloud": #uses_cloud })
                    ).expect("Failed to serialize args");
                    ::leptos::task::spawn_local(async move {
                        if let Err(e) = call_tauri("set_setting", args).await {
                            ::log::error!("Failed to save setting {}: {:?}", stringify!(#field_name), e);
                        }
                    });
                }

                pub async fn #get_fn(&self) -> Result<(), String> {
                    let signal = self.#field_name.val;
                    let args = ::serde_wasm_bindgen::to_value(
                        &::serde_json::json!({ "key": #type_name, "from_cloud": #uses_cloud })
                    ).map_err(|e| format!("Failed to serialize args: {:?}", e))?;
                    let res = call_tauri("get_setting", args)
                        .await
                        .map_err(|e| format!("Tauri call failed: {:?}", e))?;
                    let json_str: Option<String> = ::serde_wasm_bindgen::from_value(res)
                        .map_err(|e| format!("Failed to deserialize response: {:?}", e))?;
                    if let Some(s) = json_str {
                        let val: #original_type = ::serde_json::from_str(&s)
                            .map_err(|e| format!("Failed to parse value: {:?}", e))?;
                        signal.set(val);
                    } else {
                        let serialized = ::serde_json::to_string(&signal.get_untracked())
                            .map_err(|e| format!("Failed to serialize default: {:?}", e))?;
                        let set_args = ::serde_wasm_bindgen::to_value(
                            &::serde_json::json!({ "key": #type_name, "value": serialized, "to_cloud": #uses_cloud })
                        ).map_err(|e| format!("Failed to serialize set args: {:?}", e))?;
                        if let Err(e) = call_tauri("set_setting", set_args).await {
                            ::log::warn!("Failed to persist default for {}: {:?}", stringify!(#field_name), e);
                        }
                    }
                    Ok(())
                }
            }
        });

    let expanded = quote! {
        #[derive(Debug)]
        pub struct MatrixSettingField<T: 'static> {
            pub val: ::leptos::prelude::RwSignal<T>,
            pub type_name: &'static str,
            pub human_readable: &'static str,
            pub uses_cloud: bool,
        }

        impl<T: 'static> Copy for MatrixSettingField<T> {}
        impl<T: 'static> Clone for MatrixSettingField<T> {
            fn clone(&self) -> Self { *self }
        }

        impl<T: 'static> MatrixSettingField<T> {
            pub fn signal(&self) -> ::leptos::prelude::RwSignal<T> {
                self.val
            }
        }

        #[derive(Clone, Copy)]
        #item

        impl Default for #struct_name {
            fn default() -> Self {
                Self {
                    #(#default_field_initializers),*
                }
            }
        }

        impl #struct_name {
            #(#accessor_methods)*

            pub async fn get_all(&self) -> Result<(), String> {
                ::log::info!("Getting all settings");
                #(#get_all_calls)*
                Ok(())
            }

            pub fn setup_backend_hook(&self) {
                let update_sig: ReadSignal<Option<::shared::api::events::SettingsUpdate>> = use_tauri_event();
                #(#signal_bindings)*

                setup_update_effect(update_sig, move |new| {
                    match new.key.as_str() {
                        #(#match_arms_file,)*
                        _ => {}
                    }

                    if new.cloud {
                        match new.key.as_str() {
                            #(#match_arms_cloud,)*
                            _ => {}
                        }
                    }
                });
            }
        }
    };

    expanded.into()
}
