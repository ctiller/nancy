use proc_macro::TokenStream;
use quote::{format_ident, quote};
use std::fs;
use std::path::PathBuf;
use syn::{
    Field, Ident, ItemStruct, LitStr, Pat, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
    spanned::Spanned,
};

pub fn md_defined(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(item as ItemStruct);

    // Transform fields of type `string` or `String` into `&'static str`
    // And remove any `#[body]` attribute
    for field in &mut ast.fields {
        let mut attrs = Vec::new();
        for attr in field.attrs.drain(..) {
            if !attr.path().is_ident("body") {
                attrs.push(attr);
            }
        }
        field.attrs = attrs;

        // Convert string types to &'static str
        let mut is_string = false;
        if let Type::Path(type_path) = &field.ty {
            if let Some(segment) = type_path.path.segments.last() {
                if segment.ident == "String" || segment.ident == "string" {
                    is_string = true;
                }
            }
        }

        if is_string {
            let str_ty: Type = syn::parse_quote!(&'static str);
            field.ty = str_ty;
        }
    }

    TokenStream::from(quote! {
        #ast
    })
}

struct IncludeMdInput {
    struct_name: Ident,
    _comma: Token![,],
    filename: LitStr,
}

impl Parse for IncludeMdInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(IncludeMdInput {
            struct_name: input.parse()?,
            _comma: input.parse()?,
            filename: input.parse()?,
        })
    }
}

pub fn include_md(input: TokenStream) -> TokenStream {
    let IncludeMdInput {
        struct_name,
        filename,
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
                    // Extract value as string
                    let v = match value {
                        serde_yaml::Value::String(s) => s,
                        serde_yaml::Value::Number(n) => n.to_string(),
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        _ => {
                            let err_msg = format!("Unsupported YAML value type for key '{}'", k);
                            return syn::Error::new(filename.span(), err_msg)
                                .to_compile_error()
                                .into();
                        }
                    };
                    let key_ident = format_ident!("{}", k);
                    fields.push(quote! { #key_ident: #v });
                }
            }
        }
    }

    // Add body field
    // We assume the body field is named `body`.
    let body_ident = format_ident!("body");
    fields.push(quote! { #body_ident: #body });

    let expanded = quote! {
        #struct_name {
            #( #fields ),*
        }
    };

    TokenStream::from(expanded)
}
