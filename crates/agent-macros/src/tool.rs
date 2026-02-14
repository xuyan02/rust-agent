use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, FnArg, ImplItem, ImplItemFn, ItemImpl, Lit, Meta, Pat, PatIdent, PathArguments,
    ReturnType, Type, parse::Parser, spanned::Spanned,
};

pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    match tool_impl(attr, item) {
        Ok(ts) => ts,
        Err(e) => e.to_compile_error().into(),
    }
}

struct ToolArgs {
    id: String,
    description: String,
}

fn parse_tool_args(attr: TokenStream) -> syn::Result<ToolArgs> {
    let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    let meta = parser.parse(attr)?;

    let mut id: Option<String> = None;
    let mut description: Option<String> = None;

    for m in meta {
        match m {
            Meta::NameValue(nv) => {
                if nv.path.is_ident("id") {
                    id = Some(lit_to_string(&nv.value)?);
                } else if nv.path.is_ident("description") {
                    description = Some(lit_to_string(&nv.value)?);
                } else {
                    return Err(syn::Error::new(nv.path.span(), "unknown #[tool] arg"));
                }
            }
            other => return Err(syn::Error::new(other.span(), "invalid #[tool] arg")),
        }
    }

    Ok(ToolArgs {
        id: id.ok_or_else(|| {
            syn::Error::new(proc_macro2::Span::call_site(), "missing #[tool(id=...)]")
        })?,
        description: description.unwrap_or_default(),
    })
}

fn lit_to_string(expr: &syn::Expr) -> syn::Result<String> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: Lit::Str(s), ..
        }) => Ok(s.value()),
        _ => Err(syn::Error::new(expr.span(), "expected string literal")),
    }
}

struct ToolFnArgs {
    name: String,
    description: Option<String>,
    hidden: bool,
    strict: bool,
}

fn parse_tool_fn_args(attrs: &[Attribute]) -> syn::Result<Option<ToolFnArgs>> {
    let attr = attrs.iter().find(|a| a.path().is_ident("tool_fn")).cloned();
    let Some(attr) = attr else {
        return Ok(None);
    };

    let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    let nested = parser.parse2(attr.meta.require_list()?.tokens.clone())?;

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut hidden = false;
    let mut strict = true;

    for m in nested {
        match m {
            Meta::NameValue(nv) => {
                if nv.path.is_ident("name") {
                    name = Some(lit_to_string(&nv.value)?);
                } else if nv.path.is_ident("description") {
                    description = Some(lit_to_string(&nv.value)?);
                } else if nv.path.is_ident("strict") {
                    match &nv.value {
                        syn::Expr::Lit(syn::ExprLit {
                            lit: Lit::Bool(b), ..
                        }) => {
                            strict = b.value();
                        }
                        _ => {
                            return Err(syn::Error::new(nv.value.span(), "expected bool literal"));
                        }
                    }
                } else {
                    return Err(syn::Error::new(nv.path.span(), "unknown #[tool_fn] arg"));
                }
            }
            Meta::Path(p) => {
                if p.is_ident("hidden") {
                    hidden = true;
                } else {
                    return Err(syn::Error::new(p.span(), "unknown #[tool_fn] flag"));
                }
            }
            other => return Err(syn::Error::new(other.span(), "invalid #[tool_fn] arg")),
        }
    }

    let name = name.ok_or_else(|| syn::Error::new(attr.span(), "missing #[tool_fn(name=...)]"))?;

    Ok(Some(ToolFnArgs {
        name,
        description,
        hidden,
        strict,
    }))
}

struct ToolArgAttrs {
    rename: Option<String>,
    desc: Option<String>,
    default: Option<syn::Expr>,
}

fn parse_tool_arg_attrs(attrs: &[Attribute]) -> syn::Result<ToolArgAttrs> {
    let mut out = ToolArgAttrs {
        rename: None,
        desc: None,
        default: None,
    };

    let Some(attr) = attrs.iter().find(|a| a.path().is_ident("tool_arg")) else {
        return Ok(out);
    };

    let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    let nested = parser.parse2(attr.meta.require_list()?.tokens.clone())?;

    for m in nested {
        match m {
            Meta::NameValue(nv) => {
                if nv.path.is_ident("rename") {
                    out.rename = Some(lit_to_string(&nv.value)?);
                } else if nv.path.is_ident("desc") {
                    out.desc = Some(lit_to_string(&nv.value)?);
                } else if nv.path.is_ident("default") {
                    out.default = Some(nv.value);
                } else {
                    return Err(syn::Error::new(nv.path.span(), "unknown #[tool_arg] arg"));
                }
            }
            other => return Err(syn::Error::new(other.span(), "invalid #[tool_arg] arg")),
        }
    }

    Ok(out)
}

fn is_workspace_param(ty: &Type) -> bool {
    // Accept &Path or &std::path::Path
    let Type::Reference(r) = ty else { return false };
    let Type::Path(tp) = &*r.elem else {
        return false;
    };
    let Some(seg) = tp.path.segments.last() else {
        return false;
    };
    seg.ident == "Path" && matches!(seg.arguments, PathArguments::None)
}

fn is_option(ty: &Type) -> Option<&Type> {
    let Type::Path(tp) = ty else { return None };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let arg = args.args.first()?;
    let syn::GenericArgument::Type(inner) = arg else {
        return None;
    };
    Some(inner)
}

fn schema_for_type(ty: &Type) -> syn::Result<proc_macro2::TokenStream> {
    if let Some(inner) = is_option(ty) {
        return schema_for_type(inner);
    }

    // Vec<T>
    if let Type::Path(tp) = ty
        && let Some(seg) = tp.path.segments.last()
        && seg.ident == "Vec"
    {
        let PathArguments::AngleBracketed(args) = &seg.arguments else {
            return Err(syn::Error::new(seg.span(), "unsupported Vec args"));
        };
        let arg = args
            .args
            .first()
            .ok_or_else(|| syn::Error::new(seg.span(), "Vec missing arg"))?;
        let syn::GenericArgument::Type(inner) = arg else {
            return Err(syn::Error::new(seg.span(), "Vec arg must be type"));
        };
        let inner_schema = schema_for_type(inner)?;
        return Ok(
            quote!(crate::tools::Schema::Array(crate::tools::ArraySchema {
                items: Box::new(#inner_schema),
            })),
        );
    }

    // primitives
    match ty {
        Type::Path(tp) => {
            let seg = tp
                .path
                .segments
                .last()
                .ok_or_else(|| syn::Error::new(tp.span(), "empty type path"))?;
            let t = seg.ident.to_string();
            let schema = match t.as_str() {
                "String" => quote!(crate::tools::Schema::String),
                "bool" | "Bool" => quote!(crate::tools::Schema::Boolean),
                "i64" | "i32" | "u64" | "u32" | "usize" | "isize" => {
                    quote!(crate::tools::Schema::Integer)
                }
                "f64" | "f32" => quote!(crate::tools::Schema::Number),
                _ => {
                    return Err(syn::Error::new(
                        tp.span(),
                        format!("unsupported tool arg type: {t}"),
                    ));
                }
            };
            Ok(schema)
        }
        _ => Err(syn::Error::new(ty.span(), "unsupported tool arg type")),
    }
}

fn decode_expr_for_type(
    arg_name: &str,
    ty: &Type,
    default_expr: Option<syn::Expr>,
) -> syn::Result<proc_macro2::TokenStream> {
    let lit_name = arg_name.to_string();

    if let Some(inner) = is_option(ty) {
        let inner_decode = decode_expr_for_type(arg_name, inner, None)?;
        return Ok(quote!({
            if let Some(v) = args.get(#lit_name) {
                Some(#inner_decode)
            } else {
                None
            }
        }));
    }

    // Vec<T>
    if let Type::Path(tp) = ty
        && let Some(seg) = tp.path.segments.last()
        && seg.ident == "Vec"
    {
        let PathArguments::AngleBracketed(args_ab) = &seg.arguments else {
            return Err(syn::Error::new(seg.span(), "unsupported Vec args"));
        };
        let arg = args_ab
            .args
            .first()
            .ok_or_else(|| syn::Error::new(seg.span(), "Vec missing arg"))?;
        let syn::GenericArgument::Type(inner) = arg else {
            return Err(syn::Error::new(seg.span(), "Vec arg must be type"));
        };

        // Note: for arrays we decode each element directly.
        let elem_decode = decode_value_expr_for_type(inner)?;

        let get_v = quote!(
            args.get(#lit_name)
                .ok_or_else(|| anyhow::anyhow!("tool arg missing: {}", #lit_name))?
        );
        return Ok(quote!({
            let v = #get_v;
            let arr = v
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("tool arg type mismatch: {}", #lit_name))?;
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(#elem_decode(item, #lit_name)?);
            }
            out
        }));
    }

    let get_required = quote!(
        args.get(#lit_name)
            .ok_or_else(|| anyhow::anyhow!("tool arg missing: {}", #lit_name))?
    );

    let get_optional_with_default = if let Some(def) = default_expr {
        quote!(args.get(#lit_name).unwrap_or(&serde_json::json!(#def)))
    } else {
        get_required
    };

    let decode_value = decode_value_expr_for_type(ty)?;

    Ok(quote!({
        let v = #get_optional_with_default;
        #decode_value(v, #lit_name)?
    }))
}

fn decode_value_expr_for_type(ty: &Type) -> syn::Result<proc_macro2::TokenStream> {
    // primitives only for now
    let Type::Path(tp) = ty else {
        return Err(syn::Error::new(ty.span(), "unsupported tool arg type"));
    };
    let seg = tp
        .path
        .segments
        .last()
        .ok_or_else(|| syn::Error::new(tp.span(), "empty type path"))?;
    let t = seg.ident.to_string();

    let body = match t.as_str() {
        "String" => quote!(
            v.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow::anyhow!("tool arg type mismatch: {}", name))
        ),
        "bool" | "Bool" => quote!(
            v.as_bool()
                .ok_or_else(|| anyhow::anyhow!("tool arg type mismatch: {}", name))
        ),
        "i64" | "i32" | "u64" | "u32" | "usize" | "isize" => quote!(
            v.as_i64()
                .ok_or_else(|| anyhow::anyhow!("tool arg type mismatch: {}", name))
        ),
        "f64" | "f32" => quote!(
            v.as_f64()
                .ok_or_else(|| anyhow::anyhow!("tool arg type mismatch: {}", name))
        ),
        _ => {
            return Err(syn::Error::new(
                tp.span(),
                format!("unsupported tool arg type: {t}"),
            ));
        }
    };

    Ok(quote!(|v: &serde_json::Value, name: &str| -> anyhow::Result<_> { #body }))
}

fn tool_impl(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let args = parse_tool_args(attr)?;
    let imp: ItemImpl = syn::parse(item)?;

    let self_ty = &imp.self_ty;

    // Collect tool fns.
    let mut fns: Vec<(ImplItemFn, ToolFnArgs)> = Vec::new();
    for it in &imp.items {
        let ImplItem::Fn(f) = it else { continue };
        if let Some(a) = parse_tool_fn_args(&f.attrs)? {
            fns.push((f.clone(), a));
        }
    }

    if fns.is_empty() {
        return Err(syn::Error::new(
            imp.span(),
            "#[tool] impl has no #[tool_fn] methods",
        ));
    }

    // spec() builder
    let mut spec_fns = Vec::new();
    for (f, fa) in &fns {
        if fa.hidden {
            continue;
        }

        let mut desc = fa.description.clone();
        if desc.is_none() {
            desc = doc_comment_text(&f.attrs);
        }
        let desc = desc.unwrap_or_default();

        let mut props_inserts = Vec::new();
        let mut required_pushes = Vec::new();

        for input in &f.sig.inputs {
            let FnArg::Typed(pat_ty) = input else {
                continue;
            };

            // detect workspace injection
            if is_workspace_param(&pat_ty.ty) {
                continue;
            }

            let Pat::Ident(PatIdent { ident, .. }) = &*pat_ty.pat else {
                return Err(syn::Error::new(pat_ty.pat.span(), "tool arg must be ident"));
            };

            let arg_attrs = parse_tool_arg_attrs(&pat_ty.attrs)?;
            let arg_name = arg_attrs.rename.unwrap_or_else(|| ident.to_string());

            let schema = schema_for_type(&pat_ty.ty)?;
            props_inserts.push(quote!(props.insert(#arg_name.to_string(), #schema);));

            let optional = is_option(&pat_ty.ty).is_some() || arg_attrs.default.is_some();
            if !optional {
                required_pushes.push(quote!(required.push(#arg_name.to_string());));
            }
        }

        let name = fa.name.clone();

        spec_fns.push(quote!({
            let mut props = std::collections::BTreeMap::new();
            #(#props_inserts)*

            let mut required = Vec::new();
            #(#required_pushes)*

            crate::FunctionSpec {
                name: #name.to_string(),
                description: #desc.to_string(),
                parameters: crate::tools::Schema::Object(crate::tools::ObjectSchema {
                    properties: props,
                    required,
                    additional_properties: false,
                }),
            }
        }));
    }

    // invoke() match arms
    let mut match_arms = Vec::new();
    for (f, fa) in &fns {
        let fn_name = fa.name.clone();
        let rust_ident = &f.sig.ident;

        let mut call_args = Vec::new();
        let mut decodes = Vec::new();

        let mut seen_workspace = false;

        for input in &f.sig.inputs {
            match input {
                FnArg::Receiver(_) => {}
                FnArg::Typed(pat_ty) => {
                    if is_workspace_param(&pat_ty.ty) {
                        if seen_workspace {
                            return Err(syn::Error::new(
                                pat_ty.ty.span(),
                                "duplicate workspace param",
                            ));
                        }
                        seen_workspace = true;
                        call_args.push(quote!(workspace));
                        continue;
                    }

                    let Pat::Ident(PatIdent { ident, .. }) = &*pat_ty.pat else {
                        return Err(syn::Error::new(pat_ty.pat.span(), "tool arg must be ident"));
                    };

                    let arg_attrs = parse_tool_arg_attrs(&pat_ty.attrs)?;
                    let arg_name = arg_attrs.rename.unwrap_or_else(|| ident.to_string());

                    let local = format_ident!("__tool_arg_{}", ident);
                    let decode = decode_expr_for_type(&arg_name, &pat_ty.ty, arg_attrs.default)?;

                    decodes.push(quote!(let #local = #decode;));
                    call_args.push(quote!(#local));
                }
            }
        }

        // strict additionalProperties=false: reject unknown keys in args
        let strict = fa.strict;
        let mut allowed_keys = Vec::new();
        for input in &f.sig.inputs {
            let FnArg::Typed(pat_ty) = input else {
                continue;
            };
            if is_workspace_param(&pat_ty.ty) {
                continue;
            }
            let Pat::Ident(PatIdent { ident, .. }) = &*pat_ty.pat else {
                continue;
            };
            let arg_attrs = parse_tool_arg_attrs(&pat_ty.attrs)?;
            let arg_name = arg_attrs.rename.unwrap_or_else(|| ident.to_string());
            allowed_keys.push(arg_name);
        }

        let unknown_check = if strict {
            quote!({
                let obj = args
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("tool arg type mismatch: args"))?;
                for k in obj.keys() {
                    if ![#(#allowed_keys),*].contains(&k.as_str()) {
                        anyhow::bail!("tool arg unknown: {}", k);
                    }
                }
            })
        } else {
            quote!({})
        };

        // Ensure async
        if f.sig.asyncness.is_none() {
            return Err(syn::Error::new(
                f.sig.span(),
                "#[tool_fn] method must be async",
            ));
        }

        // Ensure return type Result<String>
        match &f.sig.output {
            ReturnType::Type(_, _ty) => {}
            ReturnType::Default => {
                return Err(syn::Error::new(
                    f.sig.span(),
                    "#[tool_fn] must return Result<String>",
                ));
            }
        }

        match_arms.push(quote!(
            #fn_name => {
                #unknown_check
                #(#decodes)*
                let out = self.#rust_ident(#(#call_args),*).await?;
                Ok(out)
            }
        ));
    }

    let tool_spec_ident = format_ident!("__AGENT_TOOL_SPEC_FOR_{}", type_ident_string(self_ty)?);

    let id = args.id;
    let description = args.description;

    let expanded = quote! {
        #imp

        #[async_trait::async_trait(?Send)]
        impl crate::Tool for #self_ty {
            fn spec(&self) -> &crate::ToolSpec {
                static #tool_spec_ident: std::sync::OnceLock<crate::ToolSpec> = std::sync::OnceLock::new();
                #tool_spec_ident.get_or_init(|| crate::ToolSpec {
                    id: #id.to_string(),
                    description: #description.to_string(),
                    functions: vec![
                        #(#spec_fns),*
                    ],
                })
            }

            async fn invoke(
                &self,
                workspace: &std::path::Path,
                function_name: &str,
                args: &serde_json::Value,
            ) -> anyhow::Result<String> {
                match function_name {
                    #(#match_arms,)*
                    _ => anyhow::bail!("unknown function: {}", function_name),
                }
            }
        }
    };

    Ok(expanded.into())
}

fn type_ident_string(ty: &Type) -> syn::Result<String> {
    match ty {
        Type::Path(tp) => Ok(tp
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_else(|| "Tool".to_string())),
        _ => Err(syn::Error::new(ty.span(), "unsupported self type")),
    }
}

fn doc_comment_text(attrs: &[Attribute]) -> Option<String> {
    let mut out = String::new();
    for a in attrs {
        if !a.path().is_ident("doc") {
            continue;
        }
        if let Meta::NameValue(nv) = &a.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: Lit::Str(s), ..
            }) = &nv.value
        {
            let line = s.value();
            let line = line.trim();
            if !line.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(line);
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}
