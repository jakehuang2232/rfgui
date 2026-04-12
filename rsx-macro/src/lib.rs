use proc_macro::TokenStream;
use proc_macro2::{Delimiter, Span, TokenTree};
use quote::{ToTokens, format_ident, quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Expr, Fields, FnArg, Ident, ItemFn, ItemStruct, Lit, LitStr, Pat, PatIdent, Path, Result,
    ReturnType, Stmt, Token, Type, TypePath, braced, parse_quote,
};

#[proc_macro]
pub fn rsx(input: TokenStream) -> TokenStream {
    let nodes = match syn::parse::<MultipleNodes>(input) {
        Ok(m) => m.nodes,
        Err(err) => return err.to_compile_error().into(),
    };

    let body = if nodes.len() == 1 {
        expand_node(&nodes[0])
    } else {
        let children = nodes.iter().map(expand_node);
        quote! {
            ::rfgui::ui::RsxNode::fragment(vec![
                #(#children),*
            ])
        }
    };

    quote! {
        ::rfgui::ui::build_scope(|| {
            #body
        })
    }
    .into()
}

struct MultipleNodes {
    nodes: Vec<Child>,
}

impl Parse for MultipleNodes {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut nodes = Vec::new();
        while !input.is_empty() {
            if input.peek(Token![<]) && !input.peek2(Token![/]) {
                nodes.push(Child::Element(input.parse()?));
            } else if input.peek(LitStr) {
                nodes.push(Child::TextLiteral(input.parse()?));
            } else if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                nodes.push(Child::Expr(content.parse()?));
            } else {
                let raw = parse_raw_text(input)?;
                if !raw.is_empty() {
                    nodes.push(Child::TextRaw(raw));
                }
            }
        }
        Ok(MultipleNodes { nodes })
    }
}

#[proc_macro_attribute]
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = syn::parse_macro_input!(item as ItemFn);
    expand_component(input_fn).into()
}

#[proc_macro_attribute]
pub fn props(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_struct = syn::parse_macro_input!(item as ItemStruct);
    expand_prop(input_struct).into()
}

#[derive(Clone)]
struct ElementNode {
    tag: Path,
    close_tag: Path,
    props: Vec<Prop>,
    children: Vec<Child>,
    diagnostics: Vec<proc_macro2::TokenStream>,
}

#[derive(Clone)]
struct Prop {
    key: Ident,
    value: PropValueExpr,
}

#[derive(Clone)]
enum PropValueExpr {
    Expr(Expr),
    Macro(proc_macro2::TokenStream),
    StyleObject(Vec<StyleEntry>),
    Object(Vec<ObjectEntry>),
    Missing,
    Invalid(proc_macro2::TokenStream),
}

#[derive(Clone)]
enum StyleValueExpr {
    Expr(Expr),
    StyleObject(Vec<StyleEntry>),
    Missing,
}

#[derive(Clone)]
struct StyleEntry {
    key: Ident,
    value: StyleValueExpr,
}

#[derive(Clone)]
struct ObjectEntry {
    key: Ident,
    value: ObjectValueExpr,
}

#[derive(Clone)]
enum ObjectValueExpr {
    Expr(Expr),
    StyleObject(Vec<StyleEntry>),
    Object(Vec<ObjectEntry>),
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
        let mut diagnostics = Vec::new();
        while !input.peek(Token![>]) && !(input.peek(Token![/]) && input.peek2(Token![>])) {
            if input.is_empty() || input.peek(Token![<]) {
                diagnostics.push(
                    syn::Error::new(tag.span(), "expected `>` to finish the start tag")
                        .to_compile_error(),
                );
                break;
            }
            let key: Ident = input.parse()?;
            if input.peek(Token![:]) {
                let colon: Token![:] = input.parse()?;
                return Err(syn::Error::new(
                    colon.spans[0],
                    format!(
                        "invalid prop syntax on `{}`: use `=` for props (for example `{}={{expr}}`).",
                        key, key
                    ),
                ));
            }
            if input.peek(Token![!]) {
                let bang: Token![!] = input.parse()?;
                let macro_body: TokenTree = input.parse()?;
                let TokenTree::Group(group) = macro_body else {
                    return Err(syn::Error::new(
                        bang.span(),
                        format!("expected delimiter group after `{}!`", key),
                    ));
                };
                let delimiter = group.delimiter();
                if delimiter == Delimiter::None {
                    return Err(syn::Error::new(
                        group.span(),
                        format!("expected delimiter group after `{}!`", key),
                    ));
                }
                let macro_tokens = quote! { #key ! #group };
                props.push(Prop {
                    key,
                    value: PropValueExpr::Macro(macro_tokens),
                });
                continue;
            }
            if !input.peek(Token![=]) {
                if can_recover_incomplete_prop(input) {
                    props.push(Prop {
                        key,
                        value: PropValueExpr::Missing,
                    });
                    continue;
                }
                return Err(syn::Error::new(
                    input.span(),
                    format!("expected `=` after prop `{}`", key),
                ));
            }
            input.parse::<Token![=]>()?;
            let value: PropValueExpr = if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                match parse_prop_value_expr(&key, &content) {
                    Ok(value) => value,
                    Err(err) => {
                        diagnostics.push(err.to_compile_error());
                        PropValueExpr::Invalid(quote_spanned! {key.span()=>
                            compile_error!("invalid prop value");
                        })
                    }
                }
            } else {
                let lit: Lit = input.parse()?;
                PropValueExpr::Expr(parse_quote!(#lit))
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
                diagnostics,
            });
        }

        if input.peek(Token![>]) {
            input.parse::<Token![>]>()?;
        } else {
            diagnostics.push(
                syn::Error::new(tag.span(), "expected `>` to finish the start tag")
                    .to_compile_error(),
            );
            if input.is_empty() {
                return Ok(Self {
                    tag: tag.clone(),
                    close_tag: tag.clone(),
                    props,
                    children: Vec::new(),
                    diagnostics,
                });
            }
        }

        let mut children = Vec::new();
        while !input.is_empty() && !(input.peek(Token![<]) && input.peek2(Token![/])) {
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

        if input.is_empty() {
            diagnostics.push(
                syn::Error::new(
                    tag.span(),
                    format!("missing closing tag for `<{}>`", tag.to_token_stream()),
                )
                .to_compile_error(),
            );
            return Ok(Self {
                tag: tag.clone(),
                close_tag: tag.clone(),
                props,
                children,
                diagnostics,
            });
        }

        input.parse::<Token![<]>()?;
        input.parse::<Token![/]>()?;
        let close_tag: Path = input.parse()?;
        if path_key(&close_tag) != path_key(&tag) {
            return Err(syn::Error::new(
                close_tag.span(),
                "closing tag does not match",
            ));
        }
        if input.peek(Token![>]) {
            input.parse::<Token![>]>()?;
        } else {
            diagnostics.push(
                syn::Error::new(close_tag.span(), "expected `>` after closing tag")
                    .to_compile_error(),
            );
        }

        Ok(Self {
            tag,
            close_tag,
            props,
            children,
            diagnostics,
        })
    }
}

fn can_recover_incomplete_prop(input: ParseStream) -> bool {
    input.peek(Token![>]) || (input.peek(Token![/]) && input.peek2(Token![>])) || input.peek(Ident)
}

fn parse_prop_value_expr(key: &Ident, input: ParseStream) -> Result<PropValueExpr> {
    if key == "style" && input.peek(syn::token::Brace) {
        let style_content;
        braced!(style_content in input);
        let entries = parse_style_entries(&style_content)?;
        if !input.is_empty() {
            return Err(syn::Error::new(input.span(), "style object syntax error"));
        }
        return Ok(PropValueExpr::StyleObject(entries));
    }

    let object_tokens: proc_macro2::TokenStream = input.fork().parse()?;
    if let Some(inner_tokens) = unwrap_single_brace_group(&object_tokens)
        && let Ok(entries) = parse_object_entries_from_tokens(inner_tokens)
    {
        let _: proc_macro2::TokenStream = input.parse()?;
        return Ok(PropValueExpr::Object(entries));
    }

    if let Ok(entries) = parse_object_entries_from_tokens(object_tokens.clone()) {
        let _: proc_macro2::TokenStream = input.parse()?;
        return Ok(PropValueExpr::Object(entries));
    }

    if input.peek(syn::token::Brace) {
        let fork = input.fork();
        let nested;
        braced!(nested in fork);
        let nested_tokens: proc_macro2::TokenStream = nested.parse()?;
        if let Ok(_entries) = parse_object_entries_from_tokens(nested_tokens)
            && fork.is_empty()
        {
            let object_content;
            braced!(object_content in input);
            let object_tokens: proc_macro2::TokenStream = object_content.parse()?;
            let entries = parse_object_entries_from_tokens(object_tokens)?;
            if !input.is_empty() {
                return Err(syn::Error::new(input.span(), "object syntax error"));
            }
            return Ok(PropValueExpr::Object(entries));
        }
    }

    let expr_tokens: proc_macro2::TokenStream = input.parse()?;
    match syn::parse2::<Expr>(expr_tokens.clone()) {
        Ok(expr) => Ok(PropValueExpr::Expr(expr)),
        Err(parse_err) => {
            if let Some(assign_span) = prop_assignment_like_span(&expr_tokens) {
                return Err(syn::Error::new(
                    assign_span,
                    format!(
                        "syntax error inside prop `{}`: `{{...}}` must be a valid Rust expression or RSX object. It looks like field assignment (`name=...`). Use `name: value` for RSX objects.",
                        key
                    ),
                ));
            }
            let mut err = syn::Error::new(
                parse_err.span(),
                format!(
                    "invalid Rust expression for prop `{}` inside `{{...}}`",
                    key
                ),
            );
            err.combine(parse_err);
            Err(err)
        }
    }
}

fn prop_assignment_like_span(tokens: &proc_macro2::TokenStream) -> Option<Span> {
    let mut iter = tokens.clone().into_iter().peekable();
    while let Some(token) = iter.next() {
        let TokenTree::Ident(_) = token else {
            continue;
        };
        let Some(next) = iter.peek() else {
            continue;
        };
        if let TokenTree::Punct(punct) = next {
            if punct.as_char() == '=' {
                return Some(punct.span());
            }
        }
    }
    None
}

fn parse_object_entries(input: ParseStream) -> Result<Vec<ObjectEntry>> {
    let mut entries = Vec::new();
    while !input.is_empty() {
        let key: Ident = input.parse()?;
        if input.peek(Token![=]) {
            let eq: Token![=] = input.parse()?;
            return Err(syn::Error::new(
                eq.spans[0],
                format!(
                    "invalid object syntax on `{}`: use `:` inside RSX objects (for example `{}: value`).",
                    key, key
                ),
            ));
        }
        input.parse::<Token![:]>()?;
        let value = if key == "style" && input.peek(syn::token::Brace) {
            let nested;
            braced!(nested in input);
            ObjectValueExpr::StyleObject(parse_style_entries(&nested)?)
        } else if input.peek(syn::token::Brace) {
            let nested;
            braced!(nested in input);
            let nested_tokens: proc_macro2::TokenStream = nested.parse()?;
            ObjectValueExpr::Object(parse_object_entries_from_tokens(nested_tokens)?)
        } else {
            ObjectValueExpr::Expr(input.parse()?)
        };
        entries.push(ObjectEntry { key, value });
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }
    Ok(entries)
}

fn parse_object_entries_from_tokens(tokens: proc_macro2::TokenStream) -> Result<Vec<ObjectEntry>> {
    struct ObjectEntries {
        entries: Vec<ObjectEntry>,
    }

    impl Parse for ObjectEntries {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                entries: parse_object_entries(input)?,
            })
        }
    }

    syn::parse2::<ObjectEntries>(tokens).map(|parsed| parsed.entries)
}

fn unwrap_single_brace_group(
    tokens: &proc_macro2::TokenStream,
) -> Option<proc_macro2::TokenStream> {
    let mut iter = tokens.clone().into_iter();
    let TokenTree::Group(group) = iter.next()? else {
        return None;
    };
    if group.delimiter() != Delimiter::Brace || iter.next().is_some() {
        return None;
    }
    Some(group.stream())
}

fn parse_style_entries(input: ParseStream) -> Result<Vec<StyleEntry>> {
    let mut entries = Vec::new();
    while !input.is_empty() {
        let style_key: Ident = input.parse()?;
        if input.peek(Token![=]) {
            let eq: Token![=] = input.parse()?;
            return Err(syn::Error::new(
                eq.spans[0],
                format!(
                    "invalid style syntax on `{}`: use `:` inside style objects (for example `{}: value`).",
                    style_key, style_key
                ),
            ));
        }
        if !input.peek(Token![:]) {
            if can_recover_incomplete_style_entry(input) {
                entries.push(StyleEntry {
                    key: style_key,
                    value: StyleValueExpr::Missing,
                });
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }
            return Err(syn::Error::new(
                input.span(),
                format!("expected `:` after style key `{}`", style_key),
            ));
        }
        input.parse::<Token![:]>()?;
        let style_value = if matches!(style_key.to_string().as_str(), "hover" | "selection")
            && input.peek(syn::token::Brace)
        {
            let nested;
            braced!(nested in input);
            StyleValueExpr::StyleObject(parse_style_entries(&nested)?)
        } else {
            StyleValueExpr::Expr(input.parse()?)
        };
        entries.push(StyleEntry {
            key: style_key,
            value: style_value,
        });
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }
    Ok(entries)
}

fn can_recover_incomplete_style_entry(input: ParseStream) -> bool {
    input.is_empty() || input.peek(Token![,])
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
    let close_tag = &element.close_tag;
    let has_children = !element.children.is_empty();
    let child_appends = element.children.iter().map(expand_child_append);
    let diagnostics = &element.diagnostics;

    let prop_schema_checks = element
        .props
        .iter()
        .filter(|p| p.key != "key")
        .map(expand_builder_prop_schema_check);
    let builder_assignments = element
        .props
        .iter()
        .filter(|p| p.key != "key")
        .map(|prop| expand_builder_assignment(&element.tag, prop));
    let component_key = component_key_tokens(element);
    let children_schema_check = if has_children {
        quote! {
            let _: [(); 1] = [(); <#tag as ::rfgui::ui::RsxChildrenPolicy>::ACCEPTS_CHILDREN as usize];
        }
    } else {
        quote! {}
    };
    let children_value = if has_children {
        quote! {{
            let mut __rsx_children = ::std::vec::Vec::new();
            #(#child_appends)*
            __rsx_children
        }}
    } else {
        quote! { ::std::vec::Vec::new() }
    };
    quote! {
        {
            #(#diagnostics)*
            let _ = ::core::marker::PhantomData::<#close_tag>;
            fn __rsx_builder_for_props<__RsxComponentProps>() -> <__RsxComponentProps as ::rfgui::ui::RsxPropsBuilder>::Builder
            where
                __RsxComponentProps: ::rfgui::ui::RsxPropsBuilder,
                #tag: ::rfgui::ui::RsxTag<__RsxComponentProps>,
            {
                <__RsxComponentProps as ::rfgui::ui::RsxPropsBuilder>::builder()
            }
            fn __rsx_build_props<__RsxComponentProps>(
                builder: <__RsxComponentProps as ::rfgui::ui::RsxPropsBuilder>::Builder,
            ) -> ::core::result::Result<__RsxComponentProps, ::std::string::String>
            where
                __RsxComponentProps: ::rfgui::ui::RsxPropsBuilder,
                #tag: ::rfgui::ui::RsxTag<__RsxComponentProps>,
            {
                <__RsxComponentProps as ::rfgui::ui::RsxPropsBuilder>::build(builder)
            }
            let mut __rsx_props_builder = __rsx_builder_for_props::<_>();
            #(#prop_schema_checks)*
            #children_schema_check
            #(#builder_assignments)*
            let __rsx_props = __rsx_build_props(__rsx_props_builder)
                .expect(concat!("rsx build error on <", stringify!(#tag), ">"));
            ::rfgui::ui::create_tag_element_with_key::<#tag, _, _>(
                __rsx_props,
                #children_value,
                #component_key,
            )
        }
    }
}

fn expand_child_append(child: &Child) -> proc_macro2::TokenStream {
    match child {
        Child::Expr(expr) => {
            quote! {
                ::rfgui::ui::append_rsx_child_node(&mut __rsx_children, #expr);
            }
        }
        _ => {
            let node = expand_node(child);
            quote! {
                __rsx_children.push(#node);
            }
        }
    }
}

fn component_key_tokens(element: &ElementNode) -> proc_macro2::TokenStream {
    let Some(prop) = element.props.iter().find(|p| p.key == "key") else {
        return quote! { ::core::option::Option::None };
    };
    match &prop.value {
        PropValueExpr::Expr(expr) => {
            quote! { ::core::option::Option::Some(::rfgui::ui::classify_component_key(&(#expr))) }
        }
        PropValueExpr::Macro(_) | PropValueExpr::StyleObject(_) | PropValueExpr::Object(_) => {
            quote_spanned! {prop.key.span()=>
                compile_error!("`key` must be a Rust expression")
            }
        }
        PropValueExpr::Missing => {
            quote_spanned! {prop.key.span()=>
                compile_error!("`key` is incomplete; expected `=` and a value")
            }
        }
        PropValueExpr::Invalid(error_tokens) => {
            quote! {{
                #error_tokens
                compile_error!("`key` must be a valid Rust expression");
                ::core::option::Option::None
            }}
        }
    }
}

fn expand_prop(input_struct: ItemStruct) -> proc_macro2::TokenStream {
    let struct_ident = &input_struct.ident;
    let builder_ident = format_ident!("__RsxPropsBuilderFor{}", struct_ident);
    let generics = &input_struct.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match &input_struct.fields {
        Fields::Named(named) => &named.named,
        _ => {
            return syn::Error::new(
                input_struct.fields.span(),
                "#[prop] only supports structs with named fields",
            )
            .to_compile_error();
        }
    };

    let mut default_fields = Vec::new();
    let mut builder_default_fields = Vec::new();
    let mut all_optional = true;
    let mut builder_fields = Vec::new();
    let mut builder_setters = Vec::new();
    let mut builder_prop_type_methods = Vec::new();
    let mut build_fields = Vec::new();
    for field in fields {
        let field_ident = match &field.ident {
            Some(ident) => ident,
            None => {
                return syn::Error::new(field.span(), "#[prop] field must be named")
                    .to_compile_error();
            }
        };
        let field_ty = &field.ty;
        if is_children_field(field_ident, field_ty) {
            return syn::Error::new(
                field.span(),
                "`children` is no longer a props field; declare it only as a #[component] function parameter",
            )
            .to_compile_error();
        }
        builder_fields.push(quote! {
            pub #field_ident: ::core::option::Option<#field_ty>,
        });
        builder_default_fields.push(quote! {
            #field_ident: ::core::option::Option::None,
        });
        if let Some(inner_ty) = option_inner_type(field_ty) {
            let type_method_ident = format_ident!("__rsx_prop_type_{}", field_ident);
            let object_schema_method_ident = format_ident!("__rsx_object_schema_{}", field_ident);
            builder_prop_type_methods.push(quote! {
                pub fn #type_method_ident(&self) -> ::core::marker::PhantomData<#inner_ty> {
                    ::core::marker::PhantomData
                }

                pub fn #object_schema_method_ident<F>(&self, _: F)
                where
                    F: ::core::ops::FnOnce(&#inner_ty),
                {}
            });
            if is_style_type(inner_ty) {
                let style_schema_method_ident = format_ident!("__rsx_style_schema_{}", field_ident);
                let selection_schema_method_ident =
                    format_ident!("__rsx_style_selection_schema_{}", field_ident);
                builder_prop_type_methods.push(quote! {
                    pub fn #style_schema_method_ident<F>(&self, _: F)
                    where
                        #struct_ident: ::rfgui::ui::RsxPropsStyleSchema,
                        F: ::core::ops::FnOnce(
                            &<#struct_ident as ::rfgui::ui::RsxPropsStyleSchema>::StyleSchema
                        ),
                    {}

                    pub fn #selection_schema_method_ident<F>(&self, _: F)
                    where
                        #struct_ident: ::rfgui::ui::RsxPropsStyleSchema,
                        F: ::core::ops::FnOnce(
                            &<<#struct_ident as ::rfgui::ui::RsxPropsStyleSchema>::StyleSchema as ::rfgui::ui::RsxStyleSchema>::SelectionSchema
                        ),
                    {}
                });
            }
            default_fields.push(quote! { #field_ident: ::core::option::Option::None, });
            if is_fn_pointer_type(inner_ty) {
                builder_setters.push(quote! {
                    pub fn #field_ident(&mut self, value: #inner_ty) {
                        self.#field_ident = ::core::option::Option::Some(::core::option::Option::Some(value));
                    }
                });
            } else {
                builder_setters.push(quote! {
                    pub fn #field_ident<V>(&mut self, value: V)
                    where
                        V: ::rfgui::ui::IntoOptionalProp<#inner_ty>,
                    {
                        self.#field_ident = ::core::option::Option::Some(value.into_optional_prop());
                    }
                });
            }
            build_fields.push(quote! {
                #field_ident: builder.#field_ident.unwrap_or(::core::option::Option::None),
            });
        } else {
            let type_method_ident = format_ident!("__rsx_prop_type_{}", field_ident);
            let object_schema_method_ident = format_ident!("__rsx_object_schema_{}", field_ident);
            builder_prop_type_methods.push(quote! {
                pub fn #type_method_ident(&self) -> ::core::marker::PhantomData<#field_ty> {
                    ::core::marker::PhantomData
                }

                pub fn #object_schema_method_ident<F>(&self, _: F)
                where
                    F: ::core::ops::FnOnce(&#field_ty),
                {}
            });
            if is_style_type(field_ty) {
                let style_schema_method_ident = format_ident!("__rsx_style_schema_{}", field_ident);
                let selection_schema_method_ident =
                    format_ident!("__rsx_style_selection_schema_{}", field_ident);
                builder_prop_type_methods.push(quote! {
                    pub fn #style_schema_method_ident<F>(&self, _: F)
                    where
                        #struct_ident: ::rfgui::ui::RsxPropsStyleSchema,
                        F: ::core::ops::FnOnce(
                            &<#struct_ident as ::rfgui::ui::RsxPropsStyleSchema>::StyleSchema
                        ),
                    {}

                    pub fn #selection_schema_method_ident<F>(&self, _: F)
                    where
                        #struct_ident: ::rfgui::ui::RsxPropsStyleSchema,
                        F: ::core::ops::FnOnce(
                            &<<#struct_ident as ::rfgui::ui::RsxPropsStyleSchema>::StyleSchema as ::rfgui::ui::RsxStyleSchema>::SelectionSchema
                        ),
                    {}
                });
            }
            all_optional = false;
            if is_fn_pointer_type(field_ty) {
                builder_setters.push(quote! {
                    pub fn #field_ident(&mut self, value: #field_ty) {
                        self.#field_ident = ::core::option::Option::Some(value);
                    }
                });
            } else {
                builder_setters.push(quote! {
                    pub fn #field_ident<V>(&mut self, value: V)
                    where
                        V: ::core::convert::Into<#field_ty>,
                    {
                        self.#field_ident = ::core::option::Option::Some(value.into());
                    }
                });
            }
            let field_name = field_ident.to_string();
            build_fields.push(quote! {
                #field_ident: builder.#field_ident.ok_or_else(|| {
                    format!("missing required prop `{}` on <{}>", #field_name, stringify!(#struct_ident))
                })?,
            });
        }
    }
    let optional_default_impl = if all_optional {
        quote! {
            impl #impl_generics ::rfgui::ui::OptionalDefault for #struct_ident #ty_generics #where_clause {
                fn optional_default() -> Self {
                    Self {
                        #(#default_fields)*
                    }
                }
            }
        }
    } else {
        quote! {}
    };
    quote! {
        #input_struct

        #[doc(hidden)]
        pub struct #builder_ident #generics {
            #(#builder_fields)*
        }

        impl #impl_generics ::core::default::Default for #builder_ident #ty_generics #where_clause {
            fn default() -> Self {
                Self {
                    #(#builder_default_fields)*
                }
            }
        }

        impl #impl_generics #builder_ident #ty_generics #where_clause {
            #(#builder_setters)*
            #(#builder_prop_type_methods)*
        }

        impl #impl_generics ::rfgui::ui::RsxPropsBuilder for #struct_ident #ty_generics #where_clause {
            type Builder = #builder_ident #ty_generics;

            fn builder() -> Self::Builder {
                ::core::default::Default::default()
            }

            fn build(builder: Self::Builder) -> ::core::result::Result<Self, ::std::string::String> {
                Ok(Self {
                    #(#build_fields)*
                })
            }
        }

        #optional_default_impl
    }
}

fn option_inner_type(ty: &Type) -> Option<&Type> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return None;
    };
    let last = path.segments.last()?;
    if last.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let first = args.args.first()?;
    let syn::GenericArgument::Type(inner_ty) = first else {
        return None;
    };
    Some(inner_ty)
}

fn is_children_field(field_ident: &Ident, field_ty: &Type) -> bool {
    if field_ident != "children" {
        return false;
    }
    let Type::Path(TypePath { qself: None, path }) = field_ty else {
        return false;
    };
    let Some(last) = path.segments.last() else {
        return false;
    };
    if last.ident != "Vec" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return false;
    };
    let Some(syn::GenericArgument::Type(Type::Path(TypePath { qself: None, path }))) =
        args.args.first()
    else {
        return false;
    };
    path.segments.last().is_some_and(|s| s.ident == "RsxNode")
}

fn is_fn_pointer_type(ty: &Type) -> bool {
    matches!(ty, Type::BareFn(_))
}

fn is_style_type(ty: &Type) -> bool {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return false;
    };
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "Style")
}

fn expand_builder_assignment(tag: &Path, prop: &Prop) -> proc_macro2::TokenStream {
    let key_ident = &prop.key;
    if matches!(prop.value, PropValueExpr::Missing) {
        let type_method_ident = format_ident!("__rsx_prop_type_{}", key_ident);
        return quote! {
            __rsx_props_builder.#key_ident(
                ::rfgui::ui::boolean_prop_shorthand(__rsx_props_builder.#type_method_ident())
            );
        };
    }
    let value = expand_builder_value_expr(tag, prop, quote!(__rsx_props_builder));
    quote! {
        __rsx_props_builder.#key_ident(#value);
    }
}

fn expand_builder_value_expr(
    tag: &Path,
    prop: &Prop,
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    expand_prop_value_expr_for_builder(tag, &prop.key, &prop.value, builder_ident)
}

fn is_text_area_tag(tag: &Path) -> bool {
    path_key(tag).ends_with("TextArea")
}

fn is_zero_arg_closure(expr: &Expr) -> bool {
    match expr {
        Expr::Closure(closure) => closure.inputs.is_empty(),
        _ => false,
    }
}

fn event_handler_binding(
    tag: &Path,
    key: &Ident,
) -> Option<(proc_macro2::TokenStream, proc_macro2::TokenStream)> {
    let binding = match key.to_string().as_str() {
        "on_mouse_down" => (
            quote!(::rfgui::ui::into_mouse_down_handler),
            quote!(::rfgui::ui::MouseDownEvent),
        ),
        "on_mouse_up" => (
            quote!(::rfgui::ui::into_mouse_up_handler),
            quote!(::rfgui::ui::MouseUpEvent),
        ),
        "on_mouse_move" => (
            quote!(::rfgui::ui::into_mouse_move_handler),
            quote!(::rfgui::ui::MouseMoveEvent),
        ),
        "on_mouse_enter" => (
            quote!(::rfgui::ui::into_mouse_enter_handler),
            quote!(::rfgui::ui::MouseEnterEvent),
        ),
        "on_mouse_leave" => (
            quote!(::rfgui::ui::into_mouse_leave_handler),
            quote!(::rfgui::ui::MouseLeaveEvent),
        ),
        "on_click" => (
            quote!(::rfgui::ui::into_click_handler),
            quote!(::rfgui::ui::ClickEvent),
        ),
        "on_key_down" => (
            quote!(::rfgui::ui::into_key_down_handler),
            quote!(::rfgui::ui::KeyDownEvent),
        ),
        "on_key_up" => (
            quote!(::rfgui::ui::into_key_up_handler),
            quote!(::rfgui::ui::KeyUpEvent),
        ),
        "on_focus" if is_text_area_tag(tag) => (
            quote!(::rfgui::ui::into_text_area_focus_handler),
            quote!(::rfgui::ui::TextAreaFocusEvent),
        ),
        "on_focus" => (
            quote!(::rfgui::ui::into_focus_handler),
            quote!(::rfgui::ui::FocusEvent),
        ),
        "on_blur" => (
            quote!(::rfgui::ui::into_blur_handler),
            quote!(::rfgui::ui::BlurEvent),
        ),
        "on_change" => (
            quote!(::rfgui::ui::into_text_change_handler),
            quote!(::rfgui::ui::TextChangeEvent),
        ),
        _ => return None,
    };
    Some(binding)
}

fn wrap_event_expr(tag: &Path, key: &Ident, expr: &Expr) -> proc_macro2::TokenStream {
    let Some((converter, event_ty)) = event_handler_binding(tag, key) else {
        return quote! { #expr };
    };
    match expr {
        Expr::Closure(closure) if is_zero_arg_closure(expr) => {
            quote! { #converter(::rfgui::ui::no_arg_handler(#expr)) }
        }
        Expr::Closure(closure) => {
            let capture = &closure.capture;
            if let Some(input) = closure.inputs.first() {
                match input {
                    Pat::Type(_) => quote! { #converter(#expr) },
                    _ => {
                        let body = &closure.body;
                        quote! {
                            #converter(#capture |#input: &mut #event_ty| #body)
                        }
                    }
                }
            } else {
                quote! { #converter(#expr) }
            }
        }
        _ => quote! { #expr },
    }
}

fn expand_builder_prop_schema_check(prop: &Prop) -> proc_macro2::TokenStream {
    let key_ident = &prop.key;
    quote! {
        let _ = &__rsx_props_builder.#key_ident;
    }
}

fn expand_prop_value_expr_for_builder(
    tag: &Path,
    key: &Ident,
    value: &PropValueExpr,
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match value {
        PropValueExpr::Expr(value) => wrap_event_expr(tag, key, value),
        PropValueExpr::Macro(tokens) => quote! { #tokens },
        PropValueExpr::StyleObject(entries) => {
            expand_style_object_for_builder(key, entries, builder_ident)
        }
        PropValueExpr::Object(entries) => {
            expand_object_value_for_builder(key, entries, builder_ident)
        }
        PropValueExpr::Missing => quote_spanned! {key.span()=>
            compile_error!("internal rsx error: missing prop value reached value expansion");
        },
        PropValueExpr::Invalid(error_tokens) => {
            quote! {{
                #error_tokens
                unreachable!("invalid rsx prop value")
            }}
        }
    }
}

fn expand_style_object_for_builder(
    key: &Ident,
    entries: &[StyleEntry],
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let type_method_ident = format_ident!("__rsx_prop_type_{}", key);
    let assignments = entries
        .iter()
        .map(|entry| expand_style_assignment(entry, quote!(__rsx_style_builder)));
    quote! {{
        ::rfgui::ui::build_typed_prop_for(#builder_ident.#type_method_ident(), |__rsx_style_builder| {
            #(#assignments)*
        })
    }}
}

fn expand_object_value_for_builder(
    key: &Ident,
    entries: &[ObjectEntry],
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let type_method_ident = format_ident!("__rsx_prop_type_{}", key);
    let assignments = entries
        .iter()
        .map(|entry| expand_object_assignment(entry, quote!(__rsx_object_builder)));
    quote! {{
        ::rfgui::ui::build_typed_prop_for(#builder_ident.#type_method_ident(), |__rsx_object_builder| {
            #(#assignments)*
        })
    }}
}

fn expand_object_assignment(
    entry: &ObjectEntry,
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let key_ident = &entry.key;
    let value = expand_object_value_expr(key_ident, &entry.value, builder_ident.clone());
    quote! {
        let _ = &#builder_ident.#key_ident;
        #builder_ident.#key_ident(#value);
    }
}

fn expand_object_value_expr(
    key: &Ident,
    value: &ObjectValueExpr,
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match value {
        ObjectValueExpr::Expr(value) => quote! { #value },
        ObjectValueExpr::StyleObject(entries) => {
            expand_style_object_for_builder(key, entries, builder_ident)
        }
        ObjectValueExpr::Object(entries) => {
            let type_method_ident = format_ident!("__rsx_prop_type_{}", key);
            let assignments = entries
                .iter()
                .map(|entry| expand_object_assignment(entry, quote!(__rsx_nested_builder)));
            quote! {{
                ::rfgui::ui::build_typed_prop_for(#builder_ident.#type_method_ident(), |__rsx_nested_builder| {
                    #(#assignments)*
                })
            }}
        }
    }
}

fn expand_style_assignment(
    entry: &StyleEntry,
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let key_ident = &entry.key;
    let key = entry.key.to_string();
    let key_probe = quote! {
        let _ = &#builder_ident.#key_ident;
    };
    if matches!(&entry.value, StyleValueExpr::Missing) {
        return quote! {
            #key_probe
            compile_error!(concat!(
                "style key `",
                #key,
                "` is incomplete; expected `:` and a value"
            ));
        };
    }
    if let StyleValueExpr::Expr(expr) = &entry.value
        && is_string_literal_expr(expr)
        && !is_color_style_key(&key)
    {
        return quote_spanned! {entry.key.span()=>
            compile_error!("string style values are unsupported for this key; use typed values (colors are the only string exception)");
        };
    }
    let style_value_tokens = match &entry.value {
        StyleValueExpr::Expr(value) => expand_maybe_none_style_expr(value, |inner| {
            quote! {
                #builder_ident.#key_ident(#inner);
            }
        }),
        StyleValueExpr::StyleObject(entries) => {
            let type_method_ident = format_ident!("__rsx_prop_type_{}", key_ident);
            let assignments = entries
                .iter()
                .map(|nested| expand_style_assignment(nested, quote!(__rsx_nested_style_builder)));
            quote! {
                #builder_ident.#key_ident(
                    ::rfgui::ui::build_typed_prop_for(
                        #builder_ident.#type_method_ident(),
                        |__rsx_nested_style_builder| {
                            #(#assignments)*
                        }
                    )
                );
            }
        }
        StyleValueExpr::Missing => unreachable!("missing style value handled above"),
    };

    quote! {
        #key_probe
        #style_value_tokens
    }
}

fn is_color_style_key(key: &str) -> bool {
    matches!(key, "background" | "background_color" | "color")
}

fn block_single_expr(block: &syn::Block) -> Option<&Expr> {
    if block.stmts.len() != 1 {
        return None;
    }
    match &block.stmts[0] {
        Stmt::Expr(expr, None) => Some(expr),
        _ => None,
    }
}

fn is_none_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Path(path) => {
            path.qself.is_none()
                && path.path.leading_colon.is_none()
                && path.path.segments.len() == 1
                && path.path.segments[0].ident == "None"
        }
        Expr::Paren(paren) => is_none_expr(&paren.expr),
        Expr::Group(group) => is_none_expr(&group.expr),
        Expr::Block(block) => block_single_expr(&block.block)
            .map(is_none_expr)
            .unwrap_or(false),
        _ => false,
    }
}

fn expand_maybe_none_style_expr<F>(value: &Expr, expand_insert: F) -> proc_macro2::TokenStream
where
    F: Fn(&Expr) -> proc_macro2::TokenStream,
{
    if is_none_expr(value) {
        return quote! {};
    }

    fn expand_conditional_style_expr<F>(
        expr: &Expr,
        expand_insert: &F,
    ) -> Option<proc_macro2::TokenStream>
    where
        F: Fn(&Expr) -> proc_macro2::TokenStream,
    {
        if is_none_expr(expr) {
            return Some(quote! {});
        }
        if let Expr::If(expr_if) = expr {
            let then_expr = block_single_expr(&expr_if.then_branch)?;
            let cond = &expr_if.cond;
            let then_tokens = expand_conditional_style_expr(then_expr, expand_insert)?;
            if let Some((_, else_expr)) = &expr_if.else_branch {
                let else_tokens = expand_conditional_style_expr(else_expr, expand_insert)?;
                return Some(quote! {
                    if #cond {
                        #then_tokens
                    } else {
                        #else_tokens
                    }
                });
            }
            return Some(quote! {
                if #cond {
                    #then_tokens
                }
            });
        }
        if let Expr::Block(expr_block) = expr
            && let Some(inner_expr) = block_single_expr(&expr_block.block)
        {
            return expand_conditional_style_expr(inner_expr, expand_insert);
        }
        if let Expr::Match(expr_match) = expr {
            let match_expr = &expr_match.expr;
            let arms = expr_match.arms.iter().map(|arm| {
                let pat = &arm.pat;
                let guard = arm
                    .guard
                    .as_ref()
                    .map(|(_, guard_expr)| quote! { if #guard_expr });
                let body_tokens = expand_conditional_style_expr(&arm.body, expand_insert)
                    .unwrap_or_else(|| quote! { #arm.body });
                quote! {
                    #pat #guard => {
                        #body_tokens
                    }
                }
            });
            return Some(quote! {
                match #match_expr {
                    #(#arms),*
                }
            });
        }
        Some(expand_insert(expr))
    }

    if let Some(tokens) = expand_conditional_style_expr(value, &expand_insert) {
        return tokens;
    }

    expand_insert(value)
}

fn is_string_literal_expr(expr: &Expr) -> bool {
    matches!(expr, Expr::Lit(expr_lit) if matches!(&expr_lit.lit, Lit::Str(_)))
}

fn expand_node(child: &Child) -> proc_macro2::TokenStream {
    match child {
        Child::Element(element) => expand_element(element),
        Child::TextLiteral(text) => {
            quote! {
                ::rfgui::ui::RsxNode::text(#text)
            }
        }
        Child::TextRaw(text) => {
            quote! {
                ::rfgui::ui::RsxNode::text(#text)
            }
        }
        Child::Expr(expr) => {
            quote! {
                ::rfgui::ui::IntoRsxNode::into_rsx_node(#expr)
            }
        }
    }
}

fn path_key(path: &Path) -> String {
    path.to_token_stream().to_string().replace(' ', "")
}

fn expand_component(input_fn: ItemFn) -> proc_macro2::TokenStream {
    let vis = &input_fn.vis;
    let comp_name = &input_fn.sig.ident;
    let helper_name = format_ident!("__rsx_component_impl_{}", comp_name);
    let props_name = format_ident!("{}Props", comp_name);
    let builder_name = format_ident!("__RsxPropsBuilderFor{}", props_name);
    let fn_generics = &input_fn.sig.generics;
    let (impl_generics, ty_generics, where_clause) = fn_generics.split_for_impl();
    let has_generics = !input_fn.sig.generics.params.is_empty();

    let output_ty = match &input_fn.sig.output {
        ReturnType::Default => quote!(::rfgui::ui::RsxNode),
        ReturnType::Type(_, ty) => quote!(#ty),
    };

    let mut prop_fields = Vec::new();
    let mut builder_fields = Vec::new();
    let mut builder_setters = Vec::new();
    let mut builder_prop_type_methods = Vec::new();
    let mut builder_default_fields = Vec::new();
    let mut build_fields = Vec::new();
    let mut optional_default_fields = Vec::new();
    let mut all_optional = true;
    let mut helper_args = Punctuated::<FnArg, Token![,]>::new();
    let mut helper_call_args = Vec::new();
    let mut accepts_children = false;

    for arg in &input_fn.sig.inputs {
        let FnArg::Typed(pat_ty) = arg else {
            return syn::Error::new(arg.span(), "#[component] does not support method receivers")
                .to_compile_error();
        };

        let Pat::Ident(PatIdent { ident, .. }) = pat_ty.pat.as_ref() else {
            return syn::Error::new(
                pat_ty.pat.span(),
                "#[component] parameters must be simple identifiers",
            )
            .to_compile_error();
        };

        let field_ident = ident.clone();
        let ty = pat_ty.ty.as_ref().clone();

        let props_field_ty = if field_ident == "children" {
            let is_children_ty = is_vec_rsx_node(&ty);
            if !is_children_ty {
                return syn::Error::new(ty.span(), "children type must be Vec<RsxNode>")
                    .to_compile_error();
            }
            accepts_children = true;
            helper_args.push(parse_quote!(#field_ident: #ty));
            helper_call_args.push(quote!(children));
            continue;
        } else {
            ty.clone()
        };
        prop_fields.push(quote!(pub #field_ident: #props_field_ty));
        builder_fields.push(quote! {
            pub #field_ident: ::core::option::Option<#props_field_ty>
        });
        builder_default_fields.push(quote! {
            #field_ident: ::core::option::Option::None,
        });
        if let Some(inner_ty) = option_inner_type(&props_field_ty) {
            let type_method_ident = format_ident!("__rsx_prop_type_{}", field_ident);
            let object_schema_method_ident = format_ident!("__rsx_object_schema_{}", field_ident);
            builder_prop_type_methods.push(quote! {
                pub fn #type_method_ident(&self) -> ::core::marker::PhantomData<#inner_ty> {
                    ::core::marker::PhantomData
                }

                pub fn #object_schema_method_ident<F>(&self, _: F)
                where
                    F: ::core::ops::FnOnce(&#inner_ty),
                {}
            });
            if is_style_type(inner_ty) {
                let style_schema_method_ident = format_ident!("__rsx_style_schema_{}", field_ident);
                let selection_schema_method_ident =
                    format_ident!("__rsx_style_selection_schema_{}", field_ident);
                builder_prop_type_methods.push(quote! {
                    pub fn #style_schema_method_ident<F>(&self, _: F)
                    where
                        #props_name #ty_generics: ::rfgui::ui::RsxPropsStyleSchema,
                        F: ::core::ops::FnOnce(
                            &<#props_name #ty_generics as ::rfgui::ui::RsxPropsStyleSchema>::StyleSchema
                        ),
                    {}

                    pub fn #selection_schema_method_ident<F>(&self, _: F)
                    where
                        #props_name #ty_generics: ::rfgui::ui::RsxPropsStyleSchema,
                        F: ::core::ops::FnOnce(
                            &<<#props_name #ty_generics as ::rfgui::ui::RsxPropsStyleSchema>::StyleSchema as ::rfgui::ui::RsxStyleSchema>::SelectionSchema
                        ),
                    {}
                });
            }
            optional_default_fields.push(quote! {
                #field_ident: ::core::option::Option::None,
            });
            if is_fn_pointer_type(inner_ty) {
                builder_setters.push(quote! {
                    pub fn #field_ident(&mut self, value: #inner_ty) {
                        self.#field_ident = ::core::option::Option::Some(::core::option::Option::Some(value));
                    }
                });
            } else {
                builder_setters.push(quote! {
                    pub fn #field_ident<V>(&mut self, value: V)
                    where
                        V: ::rfgui::ui::IntoOptionalProp<#inner_ty>,
                    {
                        self.#field_ident = ::core::option::Option::Some(value.into_optional_prop());
                    }
                });
            }
            build_fields.push(quote! {
                #field_ident: builder.#field_ident.unwrap_or(::core::option::Option::None),
            });
        } else {
            let type_method_ident = format_ident!("__rsx_prop_type_{}", field_ident);
            let object_schema_method_ident = format_ident!("__rsx_object_schema_{}", field_ident);
            builder_prop_type_methods.push(quote! {
                pub fn #type_method_ident(&self) -> ::core::marker::PhantomData<#props_field_ty> {
                    ::core::marker::PhantomData
                }

                pub fn #object_schema_method_ident<F>(&self, _: F)
                where
                    F: ::core::ops::FnOnce(&#props_field_ty),
                {}
            });
            if is_style_type(&props_field_ty) {
                let style_schema_method_ident = format_ident!("__rsx_style_schema_{}", field_ident);
                let selection_schema_method_ident =
                    format_ident!("__rsx_style_selection_schema_{}", field_ident);
                builder_prop_type_methods.push(quote! {
                    pub fn #style_schema_method_ident<F>(&self, _: F)
                    where
                        #props_name #ty_generics: ::rfgui::ui::RsxPropsStyleSchema,
                        F: ::core::ops::FnOnce(
                            &<#props_name #ty_generics as ::rfgui::ui::RsxPropsStyleSchema>::StyleSchema
                        ),
                    {}

                    pub fn #selection_schema_method_ident<F>(&self, _: F)
                    where
                        #props_name #ty_generics: ::rfgui::ui::RsxPropsStyleSchema,
                        F: ::core::ops::FnOnce(
                            &<<#props_name #ty_generics as ::rfgui::ui::RsxPropsStyleSchema>::StyleSchema as ::rfgui::ui::RsxStyleSchema>::SelectionSchema
                        ),
                    {}
                });
            }
            all_optional = false;
            if is_fn_pointer_type(&props_field_ty) {
                builder_setters.push(quote! {
                    pub fn #field_ident(&mut self, value: #props_field_ty) {
                        self.#field_ident = ::core::option::Option::Some(value);
                    }
                });
            } else {
                builder_setters.push(quote! {
                    pub fn #field_ident<V>(&mut self, value: V)
                    where
                        V: ::core::convert::Into<#props_field_ty>,
                    {
                        self.#field_ident = ::core::option::Option::Some(value.into());
                    }
                });
            }
            let field_name = field_ident.to_string();
            build_fields.push(quote! {
                #field_ident: builder.#field_ident.ok_or_else(|| {
                    format!("missing required prop `{}` on <{}>", #field_name, stringify!(#comp_name))
                })?,
            });
        }

        helper_args.push(parse_quote!(#field_ident: #ty));
        helper_call_args.push(quote!(props.#field_ident));
    }

    let body = &input_fn.block;
    let helper_generics = &input_fn.sig.generics;

    let mut phantom_types = Vec::new();
    for param in input_fn.sig.generics.params.iter() {
        match param {
            syn::GenericParam::Type(type_param) => {
                let ident = &type_param.ident;
                phantom_types.push(quote!(#ident));
            }
            syn::GenericParam::Lifetime(lifetime_param) => {
                let lifetime = &lifetime_param.lifetime;
                phantom_types.push(quote!(&#lifetime ()));
            }
            syn::GenericParam::Const(const_param) => {
                let ident = &const_param.ident;
                phantom_types.push(quote!([(); { let _ = #ident; 0 }]));
            }
        }
    }

    let optional_default_impl = if all_optional {
        quote! {
            impl #impl_generics ::rfgui::ui::OptionalDefault for #props_name #ty_generics #where_clause {
                fn optional_default() -> Self {
                    Self {
                        #(#optional_default_fields)*
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    let component_struct_tokens = if has_generics {
        quote! {
            #vis struct #comp_name #fn_generics (
                ::core::marker::PhantomData<(#(#phantom_types),*)>
            );
        }
    } else {
        quote! {
            #vis struct #comp_name;
        }
    };

    quote! {
        #component_struct_tokens

        #vis struct #props_name #fn_generics {
            #(#prop_fields,)*
        }

        #[doc(hidden)]
        #vis struct #builder_name #fn_generics {
            #(#builder_fields,)*
        }

        impl #impl_generics ::core::default::Default for #builder_name #ty_generics #where_clause {
            fn default() -> Self {
                Self {
                    #(#builder_default_fields)*
                }
            }
        }

        impl #impl_generics #builder_name #ty_generics #where_clause {
            #(#builder_setters)*
            #(#builder_prop_type_methods)*
        }

        impl #impl_generics ::rfgui::ui::RsxPropsBuilder for #props_name #ty_generics #where_clause {
            type Builder = #builder_name #ty_generics;

            fn builder() -> Self::Builder {
                ::core::default::Default::default()
            }

            fn build(builder: Self::Builder) -> ::core::result::Result<Self, ::std::string::String> {
                Ok(Self {
                    #(#build_fields)*
                })
            }
        }

        impl #impl_generics ::rfgui::ui::RsxComponent<#props_name #ty_generics> for #comp_name #ty_generics #where_clause {
            fn render(props: #props_name #ty_generics, children: ::std::vec::Vec<::rfgui::ui::RsxNode>) -> ::rfgui::ui::RsxNode {
                let _ = &children;
                #helper_name(#(#helper_call_args),*)
            }
        }

        impl #impl_generics ::rfgui::ui::RsxChildrenPolicy for #comp_name #ty_generics #where_clause {
            const ACCEPTS_CHILDREN: bool = #accepts_children;
        }

        #[allow(non_snake_case)]
        fn #helper_name #helper_generics (#helper_args) -> #output_ty #body

        #optional_default_impl
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

#[cfg(test)]
mod tests {
    use super::{MultipleNodes, PropValueExpr, StyleValueExpr, expand_node};
    use quote::ToTokens;

    #[test]
    fn recovers_incomplete_prop_before_self_closing_tag_end() {
        let parsed = syn::parse_str::<MultipleNodes>(
            r#"<NumberField binding={bindings.number} min=0.0 max=100.0 s />"#,
        )
        .expect("rsx should recover incomplete prop");

        let node = match &parsed.nodes[0] {
            super::Child::Element(node) => node,
            _ => panic!("expected element node"),
        };
        let prop = node.props.last().expect("missing recovered prop");
        assert_eq!(prop.key.to_string(), "s");
        assert!(matches!(prop.value, PropValueExpr::Missing));
    }

    #[test]
    fn recovers_incomplete_prop_before_next_prop() {
        let parsed = syn::parse_str::<MultipleNodes>(r#"<Element s other="x" />"#)
            .expect("rsx should recover incomplete prop");

        let node = match &parsed.nodes[0] {
            super::Child::Element(node) => node,
            _ => panic!("expected element node"),
        };
        assert_eq!(node.props.len(), 2);
        assert_eq!(node.props[0].key.to_string(), "s");
        assert!(matches!(node.props[0].value, PropValueExpr::Missing));
        assert_eq!(node.props[1].key.to_string(), "other");
    }

    #[test]
    fn parses_prop_macro_invocation() {
        let parsed =
            syn::parse_str::<MultipleNodes>(r#"<Element style!{ width: Length::px(10.0) } />"#)
                .expect("rsx should parse prop macro");

        let node = match &parsed.nodes[0] {
            super::Child::Element(node) => node,
            _ => panic!("expected element node"),
        };
        let prop = node.props.first().expect("missing macro prop");
        assert_eq!(prop.key.to_string(), "style");
        let PropValueExpr::Macro(tokens) = &prop.value else {
            panic!("expected macro prop value");
        };
        assert_eq!(
            tokens.to_string(),
            "style ! { width : Length :: px (10.0) }"
        );
    }

    #[test]
    fn recovers_incomplete_style_key_before_style_object_end() {
        let parsed = syn::parse_str::<MultipleNodes>(
            r##"<Element style={{ background: Color::hex("#000"), backg }} />"##,
        )
        .expect("rsx should recover incomplete style key");

        let node = match &parsed.nodes[0] {
            super::Child::Element(node) => node,
            _ => panic!("expected element node"),
        };
        let style_prop = node
            .props
            .iter()
            .find(|prop| prop.key == "style")
            .expect("missing style prop");
        let PropValueExpr::StyleObject(entries) = &style_prop.value else {
            panic!("expected style object");
        };
        let entry = entries.last().expect("missing recovered style entry");
        assert_eq!(entry.key.to_string(), "backg");
        assert!(matches!(entry.value, StyleValueExpr::Missing));
    }

    #[test]
    fn recovers_incomplete_style_key_before_comma() {
        let parsed = syn::parse_str::<MultipleNodes>(
            r##"<Element style={{ backg, color: Color::hex("#fff") }} />"##,
        )
        .expect("rsx should recover incomplete style key");

        let node = match &parsed.nodes[0] {
            super::Child::Element(node) => node,
            _ => panic!("expected element node"),
        };
        let style_prop = node
            .props
            .iter()
            .find(|prop| prop.key == "style")
            .expect("missing style prop");
        let PropValueExpr::StyleObject(entries) = &style_prop.value else {
            panic!("expected style object");
        };
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key.to_string(), "backg");
        assert!(matches!(entries[0].value, StyleValueExpr::Missing));
        assert_eq!(entries[1].key.to_string(), "color");
    }

    #[test]
    fn recovers_missing_tag_end_and_keeps_following_sibling() {
        let parsed = syn::parse_str::<MultipleNodes>(r#"<Element foo={bar}</Element><Label />"#)
            .expect("rsx should recover missing `>` before closing tag");

        assert_eq!(parsed.nodes.len(), 2);

        let first = match &parsed.nodes[0] {
            super::Child::Element(node) => node,
            _ => panic!("expected first node to be element"),
        };
        let expanded = expand_node(&parsed.nodes[0]).to_string();
        assert_eq!(first.props.len(), 1);
        assert!(expanded.contains("expected `>` to finish the start tag"));

        match &parsed.nodes[1] {
            super::Child::Element(node) => {
                assert_eq!(node.tag.to_token_stream().to_string(), "Label");
            }
            _ => panic!("expected second node to be element"),
        }
    }

    #[test]
    fn keeps_parsing_after_invalid_prop_expression() {
        let parsed = syn::parse_str::<MultipleNodes>(
            r#"<Element on_mouse_down={resize_bottom_down />}</Element><Label />"#,
        )
        .expect("rsx should recover invalid prop expression");

        assert_eq!(parsed.nodes.len(), 2);

        let first = match &parsed.nodes[0] {
            super::Child::Element(node) => node,
            _ => panic!("expected first node to be element"),
        };
        assert!(matches!(first.props[0].value, PropValueExpr::Invalid(_)));

        let expanded = expand_node(&parsed.nodes[0]).to_string();
        assert!(
            expanded.contains("invalid Rust expression for prop `on_mouse_down` inside `{...}`")
        );

        match &parsed.nodes[1] {
            super::Child::Element(node) => {
                assert_eq!(node.tag.to_token_stream().to_string(), "Label");
            }
            _ => panic!("expected second node to be element"),
        }
    }

    #[test]
    fn text_style_expansion_uses_text_style_schema() {
        let parsed = syn::parse_str::<MultipleNodes>(
            r##"<Text style={{ color: Color::hex("#fff") }}>{"A"}</Text>"##,
        )
        .expect("rsx should parse text style");

        let expanded = expand_node(&parsed.nodes[0]).to_string();
        assert!(expanded.contains("Text"));
        assert!(expanded.contains("__rsx_style_schema_style"));
    }

    #[test]
    fn object_prop_expansion_uses_object_schema_hooks() {
        let parsed = syn::parse_str::<MultipleNodes>(
            r##"<Window window_slots={{ root_style: { background: Color::hex("#fff") } }} />"##,
        )
        .expect("rsx should parse nested object prop");

        let expanded = expand_node(&parsed.nodes[0]).to_string();
        assert!(expanded.contains("__rsx_object_schema_window_slots"));
        assert!(expanded.contains("__rsx_object_schema_root_style"));
    }

    #[test]
    fn rsx_expansion_uses_create_tag_element_path() {
        let parsed = syn::parse_str::<MultipleNodes>(r#"<Element />"#).expect("rsx should parse");

        let expanded = expand_node(&parsed.nodes[0]).to_string();
        assert!(expanded.contains("create_tag_element_with_key"));
    }
}
