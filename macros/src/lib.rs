use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Expr, ExprLit, Fields, ItemStruct, Lit, Token};

#[proc_macro_attribute]
pub fn matrix_settings(_attr: TokenStream, item: TokenStream) -> TokenStream {
    assert!(_attr.is_empty());
    let item_ast = syn::parse(item).unwrap();

    convert_settings(item_ast)
}

fn convert_settings(mut item: ItemStruct) -> TokenStream {
    let struct_name = item.ident.clone();
    let mut default_field_initializers = vec![];

    if let Fields::Named(ref mut fields) = item.fields {
        for field in &mut fields.named {
            let original_type = field.ty.clone();
            let field_name = field.ident.as_ref().unwrap();

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
                            && expr_path.path.is_ident("default") {
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
            } else {
                default_field_initializers.push(quote! {
                    #field_name: Default::default()
                });
            }
        }
    } else {
        panic!("Only applicable to structs with named fields")
    }

    let expanded = quote! {
        #[derive(Debug, Clone, Copy)]
        pub struct MatrixSettingField<T: 'static> {
            pub val: ::leptos::prelude::RwSignal<T>,
            pub type_name: &'static str,
            pub human_readable: &'static str,
            pub uses_cloud: bool,
        }

        impl<T: 'static> MatrixSettingField<T> {
            pub fn signal(&self) -> ::leptos::prelude::RwSignal<T> {
                self.val
            }
        }

        #item

        impl Default for #struct_name {
            fn default() -> Self {
                Self {
                    #(#default_field_initializers),*
                }
            }
        }
    };

    expanded.into()
}
