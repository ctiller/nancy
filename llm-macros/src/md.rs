use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::fs;
use std::path::PathBuf;
use syn::{
    Ident, ItemStruct, LitStr, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

pub fn md_defined(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(item as ItemStruct);

    let struct_ident = &ast.ident;
    let mut default_fields = Vec::new();

    // Transform fields of type `string` or `String` into `&'static str`
    // And remove any `#[body]` attribute
    for field in &mut ast.fields {
        let field_ident = field.ident.clone().unwrap();
        let mut attrs = Vec::new();
        for attr in field.attrs.drain(..) {
            if !attr.path().is_ident("body") {
                attrs.push(attr);
            }
        }
        field.attrs = attrs;

        // Convert string types to &'static str
        let mut is_string = false;
        let mut is_option = false;
        if let Type::Path(type_path) = &field.ty
            && let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "String" || segment.ident == "string" {
                    is_string = true;
                } else if segment.ident == "Option" {
                    is_option = true;
                } else if segment.ident == "PersonaCategory" {
                    default_fields.push(quote! { #field_ident: PersonaCategory::Technical });
                }
            }

        if is_string {
            default_fields.push(quote! { #field_ident: "" });
            let str_ty: Type = syn::parse_quote!(&'static str);
            field.ty = str_ty;
        } else if is_option {
            default_fields.push(quote! { #field_ident: None });
        } else {
            // If it's PersonaCategory, it already pushed above. We check so we don't push twice!
            let mut is_cat = false;
            if let Type::Path(type_path) = &field.ty
                && let Some(segment) = type_path.path.segments.last()
                    && segment.ident == "PersonaCategory" {
                        is_cat = true;
                    }
            if !is_cat {
                default_fields.push(quote! { #field_ident: Default::default() });
            }
        }
    }

    TokenStream::from(quote! {
        #ast

        impl #struct_ident {
            #[doc(hidden)]
            pub const __MD_DEFAULT: Self = Self {
                #(#default_fields),*
            };
        }
    })
}

struct IncludeMdInput {
    struct_name: Ident,
    _comma: Token![,],
    filename: LitStr,
    body_field: Option<(Token![,], Ident)>,
}

impl Parse for IncludeMdInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let struct_name = input.parse()?;
        let _comma = input.parse()?;
        let filename = input.parse()?;

        let body_field = if input.peek(Token![,]) {
            Some((input.parse()?, input.parse()?))
        } else {
            None
        };

        Ok(IncludeMdInput {
            struct_name,
            _comma,
            filename,
            body_field,
        })
    }
}

pub fn include_md(input: TokenStream) -> TokenStream {
    let IncludeMdInput {
        struct_name,
        filename,
        body_field,
        ..
    } = parse_macro_input!(input as IncludeMdInput);
    let path_str = filename.value();

    // Determine path relative to CARGO_MANIFEST_DIR
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let mut path = PathBuf::from(manifest_dir);
    path.push(path_str);

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            let err_msg = format!("Failed to read file at {:?}: {}", path, e);
            return syn::Error::new(filename.span(), err_msg)
                .to_compile_error()
                .into();
        }
    };

    // Parse the markdown string
    // Format is front matter up to `---` then body.
    let mut front_matter = String::new();
    let mut body = String::new();
    let mut in_body = false;

    for line in content.lines() {
        if !in_body && line.trim() == "---" {
            in_body = true;
            continue;
        }

        if in_body {
            body.push_str(line);
            body.push('\n');
        } else {
            front_matter.push_str(line);
            front_matter.push('\n');
        }
    }

    // Parse the yaml front matter
    let mut fields = Vec::new();
    if !front_matter.trim().is_empty() {
        let yaml_val: serde_yaml::Value = match serde_yaml::from_str(&front_matter) {
            Ok(v) => v,
            Err(e) => {
                let err_msg = format!("Failed to parse YAML front matter: {}", e);
                return syn::Error::new(filename.span(), err_msg)
                    .to_compile_error()
                    .into();
            }
        };

        if let serde_yaml::Value::Mapping(map) = yaml_val {
            for (key, value) in map {
                if let serde_yaml::Value::String(k) = key {
                    let key_ident = format_ident!("{}", k);
                    match value {
                        serde_yaml::Value::String(s) => {
                            if k == "category" {
                                let variant_ident = format_ident!("{}", s);
                                fields.push(quote! { #key_ident: PersonaCategory::#variant_ident });
                            } else {
                                fields.push(quote! { #key_ident: #s });
                            }
                        }
                        serde_yaml::Value::Number(n) => {
                            if let Some(f) = n.as_f64() {
                                let f32_val = f as f32;
                                fields.push(quote! { #key_ident: Some(#f32_val) });
                            } else if let Some(i) = n.as_i64() {
                                let f32_val = i as f32;
                                fields.push(quote! { #key_ident: Some(#f32_val) });
                            }
                        }
                        serde_yaml::Value::Bool(b) => {
                            fields.push(quote! { #key_ident: #b });
                        }
                        _ => {
                            let err_msg = format!("Unsupported YAML value type for key '{}'", k);
                            return syn::Error::new(filename.span(), err_msg)
                                .to_compile_error()
                                .into();
                        }
                    }
                }
            }
        }
    }

    // Add body field
    // We assume the body field is named `body` unless specified otherwise.
    let body_ident = if let Some((_, ident)) = body_field {
        ident
    } else {
        format_ident!("body")
    };
    fields.push(quote! { #body_ident: #body });

    let expanded = quote! {
        #struct_name {
            #( #fields, )*
            ..#struct_name::__MD_DEFAULT
        }
    };

    TokenStream::from(expanded)
}
