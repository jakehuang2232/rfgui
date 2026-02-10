use proc_macro::TokenStream;
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Expr, FnArg, Ident, ItemFn, Lit, LitStr, Pat, PatIdent, Path, Result, ReturnType, Token,
    Type, TypePath, braced, parse_quote,
};

#[proc_macro]
pub fn rsx(input: TokenStream) -> TokenStream {
    let root = syn::parse_macro_input!(input as ElementNode);
    expand_element(&root).into()
}

#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = syn::parse_macro_input!(item as ItemFn);
    expand_component(input_fn).into()
}

#[derive(Clone)]
struct ElementNode {
    tag: Path,
    close_tag: Path,
    props: Vec<Prop>,
    children: Vec<Child>,
}

#[derive(Clone)]
struct Prop {
    key: Ident,
    value: Expr,
}

#[derive(Clone)]
enum Child {
    Element(ElementNode),
    TextLiteral(LitStr),
    TextRaw(String),
    Expr(Expr),
}

impl Parse for ElementNode {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<Token![<]>()?;
        let tag: Path = input.parse()?;

        let mut props = Vec::new();
        while !input.peek(Token![>]) && !(input.peek(Token![/]) && input.peek2(Token![>])) {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: Expr = if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                content.parse()?
            } else {
                let lit: Lit = input.parse()?;
                parse_quote!(#lit)
            };
            props.push(Prop { key, value });
        }

        if input.peek(Token![/]) {
            input.parse::<Token![/]>()?;
            input.parse::<Token![>]>()?;
            return Ok(Self {
                tag: tag.clone(),
                close_tag: tag.clone(),
                props,
                children: Vec::new(),
            });
        }

        input.parse::<Token![>]>()?;

        let mut children = Vec::new();
        while !(input.peek(Token![<]) && input.peek2(Token![/])) {
            if input.peek(Token![<]) {
                children.push(Child::Element(input.parse()?));
            } else if input.peek(LitStr) {
                children.push(Child::TextLiteral(input.parse()?));
            } else if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                children.push(Child::Expr(content.parse()?));
            } else {
                let raw = parse_raw_text(input)?;
                if !raw.is_empty() {
                    children.push(Child::TextRaw(raw));
                }
            }
        }

        input.parse::<Token![<]>()?;
        input.parse::<Token![/]>()?;
        let close_tag: Path = input.parse()?;
        if path_key(&close_tag) != path_key(&tag) {
            return Err(syn::Error::new(close_tag.span(), "closing tag 不一致"));
        }
        input.parse::<Token![>]>()?;

        Ok(Self {
            tag,
            close_tag,
            props,
            children,
        })
    }
}

fn parse_raw_text(input: ParseStream) -> Result<String> {
    let mut tokens = Vec::new();

    while !input.is_empty() && !input.peek(Token![<]) && !input.peek(syn::token::Brace) {
        let token: proc_macro2::TokenTree = input.parse()?;
        tokens.push(token.to_string());
    }

    let joined = tokens.join(" ");
    Ok(normalize_text(&joined))
}

fn normalize_text(input: &str) -> String {
    let mut out = input.split_whitespace().collect::<Vec<_>>().join(" ");

    for punct in ["!", "?", ",", ".", ";", ":"] {
        let from = format!(" {}", punct);
        out = out.replace(&from, punct);
    }

    out.trim().to_string()
}

fn expand_element(element: &ElementNode) -> proc_macro2::TokenStream {
    let tag = &element.tag;
    let tag_name = tag_name(tag).unwrap_or_else(|| tag.to_token_stream().to_string());
    let close_tag = &element.close_tag;
    let has_children = !element.children.is_empty();
    let tag_span = tag.span();
    let children_schema_check = if has_children {
        quote_spanned! {tag_span=>
            let _ = |__schema: &<#tag as ::rust_gui::ui::RsxPropSchema>::PropsSchema| {
                let _ = &__schema.children;
            };
        }
    } else {
        quote! {}
    };
    let prop_statements = element.props.iter().map(|p| expand_prop_set(tag, p));
    let child_statements = element.children.iter().map(expand_child_push);

    quote! {
        {
            let _ = ::core::marker::PhantomData::<#close_tag>;
            #children_schema_check
            let mut __rsx_props = ::rust_gui::ui::RsxProps::new();
            let mut __rsx_children = Vec::<::rust_gui::ui::RsxNode>::new();
            #(#prop_statements)*
            #(#child_statements)*
            <#tag as ::rust_gui::ui::RsxTag>::rsx_render(__rsx_props, __rsx_children).unwrap_or_else(|__err| {
                panic!("rsx build error on <{}>. {}", #tag_name, __err)
            })
        }
    }
}

fn expand_prop_set(tag: &Path, prop: &Prop) -> proc_macro2::TokenStream {
    let key_ident = &prop.key;
    let key = prop.key.to_string();
    let value = &prop.value;
    quote! {
        let _ = |__schema: &<#tag as ::rust_gui::ui::RsxPropSchema>::PropsSchema| {
            let _ = &__schema.#key_ident;
        };
        __rsx_props.push(#key, ::rust_gui::ui::IntoPropValue::into_prop_value(#value));
    }
}

fn expand_child_push(child: &Child) -> proc_macro2::TokenStream {
    match child {
        Child::Element(element) => {
            let child_expr = expand_element(element);
            quote! {
                __rsx_children.push(#child_expr);
            }
        }
        Child::TextLiteral(text) => {
            quote! {
                __rsx_children.push(::rust_gui::ui::RsxNode::text(#text));
            }
        }
        Child::TextRaw(text) => {
            quote! {
                __rsx_children.push(::rust_gui::ui::RsxNode::text(#text));
            }
        }
        Child::Expr(expr) => {
            quote! {
                __rsx_children.push(::rust_gui::ui::IntoRsxNode::into_rsx_node(#expr));
            }
        }
    }
}

fn tag_name(path: &Path) -> Option<String> {
    path.segments.last().map(|seg| seg.ident.to_string())
}

fn path_key(path: &Path) -> String {
    path.to_token_stream().to_string().replace(' ', "")
}

fn expand_component(input_fn: ItemFn) -> proc_macro2::TokenStream {
    if !input_fn.sig.generics.params.is_empty() {
        return syn::Error::new(input_fn.sig.generics.span(), "#[component] 目前不支援泛型")
            .to_compile_error();
    }

    let vis = &input_fn.vis;
    let comp_name = &input_fn.sig.ident;
    let helper_name = format_ident!("__rsx_component_impl_{}", comp_name);
    let props_name = format_ident!("{}Props", comp_name);

    let output_ty = match &input_fn.sig.output {
        ReturnType::Default => quote!(::rust_gui::ui::RsxNode),
        ReturnType::Type(_, ty) => quote!(#ty),
    };

    let mut prop_fields = Vec::new();
    let mut prop_extracts = Vec::new();
    let mut helper_args = Punctuated::<FnArg, Token![,]>::new();
    let mut helper_call_args = Vec::new();
    let mut field_idents = Vec::new();
    let mut has_children_param = false;

    for arg in &input_fn.sig.inputs {
        let FnArg::Typed(pat_ty) = arg else {
            return syn::Error::new(arg.span(), "#[component] 不支援方法接收者")
                .to_compile_error();
        };

        let Pat::Ident(PatIdent { ident, .. }) = pat_ty.pat.as_ref() else {
            return syn::Error::new(pat_ty.pat.span(), "#[component] 參數必須是簡單識別字")
                .to_compile_error();
        };

        let field_ident = ident.clone();
        let ty = pat_ty.ty.as_ref().clone();

        if field_ident == "children" {
            let is_children_ty = is_vec_rsx_node(&ty);
            if !is_children_ty {
                return syn::Error::new(ty.span(), "children 型別必須是 Vec<RsxNode>")
                    .to_compile_error();
            }
            has_children_param = true;
            prop_fields.push(quote!(pub #field_ident: Vec<::rust_gui::ui::RsxNode>));
            prop_extracts.push(quote!(let #field_ident = children;));
        } else {
            prop_fields.push(quote!(pub #field_ident: #ty));
            let key = field_ident.to_string();
            let extract = build_prop_extract(&field_ident, &ty, &key);
            prop_extracts.push(extract);
        }

        helper_args.push(parse_quote!(#field_ident: #ty));
        helper_call_args.push(quote!(props.#field_ident));
        field_idents.push(field_ident);
    }

    let body = &input_fn.block;
    let children_guard = if has_children_param {
        quote! {}
    } else {
        quote! {
            if !children.is_empty() {
                return Err(format!("<{}> does not accept children", stringify!(#comp_name)));
            }
        }
    };

    quote! {
        #vis struct #comp_name;

        #vis struct #props_name {
            #(#prop_fields,)*
        }

        impl ::rust_gui::ui::FromRsxProps for #props_name {
            const ACCEPTS_CHILDREN: bool = #has_children_param;

            fn from_rsx_props(
                mut props: ::rust_gui::ui::RsxProps,
                children: Vec<::rust_gui::ui::RsxNode>,
            ) -> Result<Self, String> {
                #children_guard
                #(#prop_extracts)*
                props.reject_remaining(stringify!(#comp_name))?;
                Ok(Self {
                    #(#field_idents,)*
                })
            }
        }

        impl ::rust_gui::ui::RsxComponent for #comp_name {
            type Props = #props_name;

            fn render(props: Self::Props) -> ::rust_gui::ui::RsxNode {
                #helper_name(#(#helper_call_args),*)
            }
        }

        #[allow(non_snake_case)]
        fn #helper_name(#helper_args) -> #output_ty #body
    }
}

fn build_prop_extract(field_ident: &Ident, ty: &Type, key: &str) -> proc_macro2::TokenStream {
    if let Some(inner) = option_inner_type(ty) {
        return quote! {
            let #field_ident = props
                .remove_t::<#inner>(#key)
                .map_err(|e| format!("prop `{}` parse error: {}", #key, e))?;
        };
    }

    quote! {
        let #field_ident = props
            .remove_t::<#ty>(#key)
            .map_err(|e| format!("prop `{}` parse error: {}", #key, e))?
            .ok_or_else(|| format!("missing required prop `{}`", #key))?;
    }
}

fn is_vec_rsx_node(ty: &Type) -> bool {
    let Type::Path(TypePath { path, .. }) = ty else {
        return false;
    };
    let Some(seg) = path.segments.last() else {
        return false;
    };
    if seg.ident != "Vec" {
        return false;
    }

    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return false;
    };
    let Some(syn::GenericArgument::Type(Type::Path(inner))) = args.args.first() else {
        return false;
    };

    inner
        .path
        .segments
        .last()
        .map(|s| s.ident == "RsxNode")
        .unwrap_or(false)
}

fn option_inner_type(ty: &Type) -> Option<&Type> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };
    let seg = path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let Some(syn::GenericArgument::Type(inner)) = args.args.first() else {
        return None;
    };
    Some(inner)
}
