// Service macro implementation
//
// This module implements the service macro, which simplifies the implementation
// of a Runar service by automatically implementing the AbstractService trait and
// handling action registration.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::{
    parse_macro_input, FnArg, Ident, ImplItem, ImplItemFn, ItemImpl, Pat, PatType, ReturnType,
    Type, TypePath,
};

/// Implementation of the service macro
pub fn service_macro(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input as a struct
    let input = parse_macro_input!(item as ItemImpl);

    // Extract the struct name
    let struct_type = match &*input.self_ty {
        Type::Path(TypePath { ref path, .. }) => path.segments.last().unwrap().ident.clone(),
        _ => panic!("Service macro can only be applied to structs"),
    };

    // Extract the service attributes from the macro annotation
    let service_attrs = extract_service_attributes(attr);

    // Find all methods marked with #[action] or #[subscribe]
    let all_methods = collect_action_methods(&input);

    // Generate the trait implementation for the AbstractService trait
    let service_impl = generate_abstract_service_impl(&struct_type, &all_methods, &service_attrs);

    TokenStream::from(quote! {
        #input

        #service_impl
    })
}

/// Extract service attributes from the TokenStream
fn extract_service_attributes(attr: TokenStream) -> HashMap<String, String> {
    let mut attrs = HashMap::new();

    if attr.is_empty() {
        return attrs;
    }

    // Convert attribute tokens to a string for simple parsing
    let attr_str = attr.to_string();

    // Simple parsing of name = "value" pairs
    for pair in attr_str.split(',') {
        let parts: Vec<&str> = pair.split('=').collect();
        if parts.len() == 2 {
            let key = parts[0].trim().to_string();

            // Extract the string value between quotes
            let value_part = parts[1].trim();
            if value_part.starts_with('"') && value_part.ends_with('"') {
                let value = value_part[1..value_part.len() - 1].to_string();
                attrs.insert(key, value);
            }
        }
    }

    attrs
}

/// Collect methods marked with #[action] or #[subscribe] in the impl block
fn collect_action_methods(input: &ItemImpl) -> Vec<(Ident, &str, ImplItemFn)> {
    // Find all methods marked with #[action] or #[subscribe]
    let all_methods = input
        .items
        .iter()
        .filter_map(|item| {
            if let ImplItem::Fn(method) = item {
                let is_action = method
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("action"));
                if is_action {
                    Some((method.sig.ident.clone(), "action", method.clone()))
                } else {
                    let is_subscription = method
                        .attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("subscribe"));
                    if is_subscription {
                        Some((method.sig.ident.clone(), "subscribe", method.clone()))
                    } else {
                        None
                    }
                }
            } else {
                None
            }
        })
        .collect::<Vec<(Ident, &str, ImplItemFn)>>();

    all_methods
}

/// Extract types from a method's parameters and return type
fn extract_types_from_method(method: &ImplItemFn) -> Vec<String> {
    let mut types = Vec::new();

    // Extract parameter types
    for arg in &method.sig.inputs {
        if let FnArg::Typed(PatType { ty, pat, .. }) = arg {
            // Skip context parameter
            if let Pat::Ident(pat_ident) = &**pat {
                let param_name = pat_ident.ident.to_string();
                if param_name == "ctx" || param_name == "context" || param_name.ends_with("ctx") {
                    continue;
                }
            }

            // Get the type as a string
            let type_str = quote! { #ty }.to_string();
            types.push(type_str);
        }
    }

    // Extract return type
    if let ReturnType::Type(_, ty) = &method.sig.output {
        // Use syn AST to extract Ok type from Result<T, E>
        if let syn::Type::Path(type_path) = &**ty {
            let seg = type_path.path.segments.last();
            if let Some(seg) = seg {
                if seg.ident == "Result" {
                    if let syn::PathArguments::AngleBracketed(ref ab) = seg.arguments {
                        // Find the first type argument (the Ok type)
                        for arg in &ab.args {
                            if let syn::GenericArgument::Type(ref inner_ty) = arg {
                                let ok_type_str = quote! { #inner_ty }.to_string();
                                types.push(ok_type_str);
                                break;
                            }
                        }
                    }
                } else {
                    // Not a Result, just add the type directly
                    let return_type_str = quote! { #ty }.to_string();
                    types.push(return_type_str);
                }
            }
        } else {
            // For non-Result types, just add the type directly
            types.push(quote! { #ty }.to_string());
        }
    }

    types
}

/// Format type string to be more readable and filter out standard types
fn format_type_string(type_str: &str) -> Option<String> {
    // Remove extra spaces that quote! adds
    let mut formatted = type_str
        .replace(" >", ">")
        .replace("< ", "<")
        .replace(" , ", ", ");

    // Remove references
    if formatted.starts_with("& ") {
        formatted = formatted[2..].to_string();
    }

    // Only filter out direct primitive types; always register containers
    match formatted.as_str() {
        // Primitive types
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" | "f32" | "f64" | "bool" | "char" | "()" | "String" => None,
        _ => Some(formatted),
    }
}

/// Generate the AbstractService trait implementation
/// Ensure the struct implements Clone for proper action handler support
#[allow(clippy::cmp_owned)]
fn generate_abstract_service_impl(
    struct_type: &Ident,
    all_methods: &[(Ident, &str, ImplItemFn)],
    _service_attrs: &HashMap<String, String>,
) -> TokenStream2 {
    // Create method identifiers for action registration
    let method_registrations = all_methods.iter().map(|(method_name, method_type, _)| {
        if *method_type == "action" {
            let register_method_name = format_ident!("register_action_{}", method_name);
            quote! {
                self.#register_method_name(context_ref).await?;
            }
        } else {
            // Must be a subscription
            let register_method_name = format_ident!("register_subscription_{}", method_name);
            quote! {
                self.#register_method_name(context_ref).await?;
            }
        }
    });

    // Extract all types from methods
    let mut all_types = HashSet::new();

    for (_, _, method) in all_methods {
        let types = extract_types_from_method(method);
        for type_str in types {
            if let Some(formatted) = format_type_string(&type_str) {
                // Skip the service type itself
                if formatted != struct_type.to_string() {
                    all_types.insert(formatted);
                }
            }
        }
    }

    // Convert to a vector and sort for consistent output
    let mut sorted_types: Vec<_> = all_types.into_iter().collect();
    sorted_types.sort();

    // Create a string representation of all types (one per line) for logging
    let types_str = sorted_types.join("\n");

    // Create type identifiers for each type (used later for serializer registration)
    let type_idents = sorted_types
        .iter()
        .map(|t| {
            // Use syn::Type to support all valid Rust types, including nested generics.
            syn::parse_str::<syn::Type>(t).unwrap_or_else(|_| panic!("Failed to parse type: {t}"))
        })
        .collect::<Vec<_>>();

    // Generate logging code for collected types
    let type_collection_code = if sorted_types.is_empty() {
        // No complex types collected – generate a simple debug log
        quote! {
            context.debug("No complex types to register for this service");
        }
    } else {
        quote! {
            context.info(format!("Types used by service {}:\n    {}", stringify!(#struct_type), #types_str));
        }
    };

    // Generate debug line for the full list when registering types
    let join_debug_code = if sorted_types.is_empty() {
        quote! {
            context.debug("All types registered: []");
        }
    } else {
        quote! {
            context.debug(format!("All types registered: [{}]", [#(stringify!(#type_idents)),*].join(", ")));
        }
    };

    quote! {
        #[async_trait::async_trait]
        impl runar_node::services::abstract_service::AbstractService  for #struct_type {
            fn name(&self) -> &str {
                &self.__runar_name
            }

            fn path(&self) -> &str {
                &self.__runar_path
            }

            fn description(&self) -> &str {
                &self.__runar_description
            }

            fn version(&self) -> &str {
                &self.__runar_version
            }

            fn network_id(&self) -> Option<String> {
                self.__runar_network_id.clone()
            }

            fn set_network_id(&mut self, network_id: String) {
                self.__runar_network_id = Some(network_id);
            }

            async fn init(&self, context: runar_node::services::LifecycleContext) -> anyhow::Result<()> {
                // Create a reference to the context
                let context_ref = &context;

                // Register all action and subscription methods defined with the #[action] or #[subscribe] macro
                #(#method_registrations)*

                // Register complex types with the serializer
                Self::register_types(context_ref).await?;

                Ok(())
            }

            async fn start(&self, _context: runar_node::services::LifecycleContext) -> anyhow::Result<()> {
                Ok(())
            }

            async fn stop(&self, _context: runar_node::services::LifecycleContext) -> anyhow::Result<()> {
                Ok(())
            }
        }

        // Helper utilities inherent to the service
        impl #struct_type {
            // Helper method to register complex types with the serializer
            async fn register_types(context: &runar_node::services::LifecycleContext) -> anyhow::Result<()> {
                // Acquire a write lock on the serializer
                let mut serializer = context.serializer.write().await;

                // Log all the collected types
                #type_collection_code

                // Register each type with the serializer
                #({
                    context.debug(format!("Registering type: {}", stringify!(#type_idents)));
                })*
                // Print all types being registered for macro transparency
                #join_debug_code
                #({
                    serializer.register::<#type_idents>()?;
                })*

                Ok(())
            }
        }
    }
}
