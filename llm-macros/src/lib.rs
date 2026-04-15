// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{ExprClosure, ItemFn, LitStr, Pat, Token, parse_macro_input};
/// Parses `#[llm_tool]` on a function.
/// It keeps the function exactly as is, but generates a companion struct Named `{FnName}Tool`
/// which implements `LlmTool`.
#[proc_macro_attribute]
pub fn llm_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let vis = &input_fn.vis;
    let mut args = Vec::new();

    for arg in &input_fn.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = arg
            && let Pat::Ident(pat_ident) = &*pat_type.pat
        {
            args.push((pat_ident.ident.clone(), pat_type.ty.clone()));
        }
    }

    // Extract doc comments for description
    let mut desc_lines = Vec::new();
    for attr in &input_fn.attrs {
        if attr.path().is_ident("doc")
            && let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &nv.value
            && let syn::Lit::Str(lit_str) = &expr_lit.lit
        {
            desc_lines.push(lit_str.value().trim().to_string());
        }
    }

    let description = desc_lines.join("\n");
    // Ensure properly capitalized camel case Tool struct
    let tool_struct_str = fn_name.to_string();
    let mut camel_case = String::new();
    let mut capitalize = true;
    for c in tool_struct_str.chars() {
        if c == '_' {
            capitalize = true;
        } else if capitalize {
            camel_case.push(c.to_ascii_uppercase());
            capitalize = false;
        } else {
            camel_case.push(c);
        }
    }
    let tool_struct_name = format_ident!("{}Tool", camel_case);

    let arg_names = args.iter().map(|(id, _)| id).collect::<Vec<_>>();
    let arg_types = args.iter().map(|(_, ty)| ty).collect::<Vec<_>>();

    let is_async = input_fn.sig.asyncness.is_some();

    let call_expr = if is_async {
        quote! {
            let res = #fn_name(#( input_args.#arg_names ),*).await?;
            Ok(::serde_json::to_value(&res).unwrap_or(::serde_json::Value::Null))
        }
    } else {
        quote! {
            let res = #fn_name(#( input_args.#arg_names ),*)?;
            Ok(::serde_json::to_value(&res).unwrap_or(::serde_json::Value::Null))
        }
    };

    let expanded = quote! {
        #input_fn

        #vis struct #tool_struct_name;

        #[::async_trait::async_trait]
        impl crate::llm::tool::LlmTool for #tool_struct_name {
            fn name(&self) -> &str {
                stringify!(#fn_name)
            }

            fn description(&self) -> String {
                #description.to_string()
            }

            fn schema(&self) -> ::schemars::Schema {
                #[allow(dead_code)]
                #[derive(::serde::Deserialize, ::schemars::JsonSchema)]
                struct Args {
                    #(
                        #arg_names: #arg_types,
                    )*
                }
                ::schemars::schema_for!(Args)
            }

            async fn call(&self, args: ::serde_json::Value) -> ::anyhow::Result<::serde_json::Value> {
                #[allow(dead_code)]
                #[derive(::serde::Deserialize)]
                struct Args {
                    #(
                        #arg_names: #arg_types,
                    )*
                }

                let input_args: Args = ::serde_json::from_value(args)?;
                #call_expr
            }
        }

        #[allow(non_snake_case)]
        #vis mod #fn_name {
            pub fn tool() -> Box<dyn crate::llm::tool::LlmTool> {
                Box::new(super::#tool_struct_name)
            }
        }
    };

    TokenStream::from(expanded)
}

struct MakeToolInput {
    name: LitStr,
    description: LitStr,
    closure: ExprClosure,
}

impl Parse for MakeToolInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![,]>()?;
        let description = input.parse()?;
        input.parse::<Token![,]>()?;
        let closure = input.parse()?;
        Ok(MakeToolInput {
            name,
            description,
            closure,
        })
    }
}

/// Parses a closure and returns an instantiated LlmTool trait object via Box<dyn LlmTool>.
#[proc_macro]
pub fn make_tool(input: TokenStream) -> TokenStream {
    let MakeToolInput {
        name,
        description,
        closure,
    } = parse_macro_input!(input as MakeToolInput);

    let mut args = Vec::new();
    for pat in &closure.inputs {
        if let Pat::Type(pat_type) = pat {
            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                args.push((pat_ident.ident.clone(), pat_type.ty.clone()));
            }
        } else {
            panic!("make_tool! requires typed closure arguments (e.g., |a: i32|)");
        }
    }

    let arg_names = args.iter().map(|(id, _)| id).collect::<Vec<_>>();
    let arg_types = args.iter().map(|(_, ty)| ty).collect::<Vec<_>>();

    let closure_is_async = closure.asyncness.is_some();

    let _closure_call = if closure_is_async {
        quote! { closure(#( input_args.#arg_names ),*).await }
    } else {
        quote! { closure(#( input_args.#arg_names ),*) }
    };

    let expanded = quote! {
        {
            #[allow(dead_code)]
            #[derive(::serde::Deserialize, ::schemars::JsonSchema)]
            struct ToolArgs {
                #( #arg_names: #arg_types, )*
            }

            struct ClosureTool<F> {
                name: String,
                description: String,
                closure: F,
            }

            #[::async_trait::async_trait]
            impl<F, Fut, T> crate::llm::tool::LlmTool for ClosureTool<F>
            where
                F: Fn(#( #arg_types ),*) -> Fut + Send + Sync,
                Fut: std::future::Future<Output = ::anyhow::Result<T>> + Send,
                T: ::serde::Serialize + Send,
            {
                fn name(&self) -> &str {
                    &self.name
                }

                fn description(&self) -> String {
                    self.description.clone()
                }

                fn schema(&self) -> ::schemars::Schema {
                    ::schemars::schema_for!(ToolArgs)
                }

                async fn call(&self, args: ::serde_json::Value) -> ::anyhow::Result<::serde_json::Value> {
                    let input_args: ToolArgs = ::serde_json::from_value(args)?;
                    let res = (self.closure)(#( input_args.#arg_names ),*).await?;
                    Ok(::serde_json::to_value(&res).unwrap_or(::serde_json::Value::Null))
                }
            }

            let closure = #closure;

            let async_wrap = move |#( #arg_names: #arg_types ),*| {
                let future = closure(#( #arg_names ),*);
                async move { future.await }
            };

            Box::new(ClosureTool {
                name: #name.to_string(),
                description: #description.to_string(),
                closure: async_wrap,
            }) as Box<dyn crate::llm::tool::LlmTool>
        }
    };

    TokenStream::from(expanded)
}

mod md;
#[proc_macro_attribute]
pub fn md_defined(attr: TokenStream, item: TokenStream) -> TokenStream {
    md::md_defined(attr, item)
}

#[proc_macro]
pub fn include_md(input: TokenStream) -> TokenStream {
    md::include_md(input)
}

// DOCUMENTED_BY: [docs/adr/0020-llm-tool-bindings.md]

