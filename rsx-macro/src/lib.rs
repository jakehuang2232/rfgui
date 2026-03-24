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
        let style_value = if style_key == "hover" && input.peek(syn::token::Brace) {
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
                #tag: ::rfgui::ui::RsxComponent<__RsxComponentProps>,
            {
                <__RsxComponentProps as ::rfgui::ui::RsxPropsBuilder>::builder()
            }
            fn __rsx_build_props<__RsxComponentProps>(
                builder: <__RsxComponentProps as ::rfgui::ui::RsxPropsBuilder>::Builder,
            ) -> ::core::result::Result<__RsxComponentProps, ::std::string::String>
            where
                __RsxComponentProps: ::rfgui::ui::RsxPropsBuilder,
                #tag: ::rfgui::ui::RsxComponent<__RsxComponentProps>,
            {
                <__RsxComponentProps as ::rfgui::ui::RsxPropsBuilder>::build(builder)
            }
            let mut __rsx_props_builder = __rsx_builder_for_props::<_>();
            #(#prop_schema_checks)*
            #children_schema_check
            #(#builder_assignments)*
            let __rsx_props = __rsx_build_props(__rsx_props_builder)
                .expect(concat!("rsx build error on <", stringify!(#tag), ">"));
            ::rfgui::ui::create_element_with_key(
                ::core::marker::PhantomData::<#tag>,
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
        PropValueExpr::StyleObject(_) | PropValueExpr::Object(_) => {
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
            builder_prop_type_methods.push(quote! {
                pub fn #type_method_ident(&self) -> ::core::marker::PhantomData<#inner_ty> {
                    ::core::marker::PhantomData
                }
            });
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
            builder_prop_type_methods.push(quote! {
                pub fn #type_method_ident(&self) -> ::core::marker::PhantomData<#field_ty> {
                    ::core::marker::PhantomData
                }
            });
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
    let value = expand_prop_value_expr_for_builder(tag, &prop.key, &prop.value, builder_ident);
    match &prop.value {
        PropValueExpr::Expr(expr) => wrap_event_expr(tag, &prop.key.to_string(), expr, value),
        _ => value,
    }
}

fn is_text_area_tag(tag: &Path) -> bool {
    path_key(tag).ends_with("TextArea")
}

fn event_handler_converter(tag: &Path, prop_key: &str) -> Option<proc_macro2::TokenStream> {
    match prop_key {
        "on_mouse_down" => Some(quote! { ::rfgui::ui::into_mouse_down_handler }),
        "on_mouse_up" => Some(quote! { ::rfgui::ui::into_mouse_up_handler }),
        "on_mouse_move" => Some(quote! { ::rfgui::ui::into_mouse_move_handler }),
        "on_mouse_enter" => Some(quote! { ::rfgui::ui::into_mouse_enter_handler }),
        "on_mouse_leave" => Some(quote! { ::rfgui::ui::into_mouse_leave_handler }),
        "on_click" => Some(quote! { ::rfgui::ui::into_click_handler }),
        "on_key_down" => Some(quote! { ::rfgui::ui::into_key_down_handler }),
        "on_key_up" => Some(quote! { ::rfgui::ui::into_key_up_handler }),
        "on_focus" if is_text_area_tag(tag) => {
            Some(quote! { ::rfgui::ui::into_text_area_focus_handler })
        }
        "on_focus" => Some(quote! { ::rfgui::ui::into_focus_handler }),
        "on_blur" => Some(quote! { ::rfgui::ui::into_blur_handler }),
        _ => None,
    }
}

fn event_handler_wrapper(tag: &Path, prop_key: &str) -> Option<proc_macro2::TokenStream> {
    match prop_key {
        "on_mouse_down" => Some(quote! { ::rfgui::ui::on_mouse_down }),
        "on_mouse_up" => Some(quote! { ::rfgui::ui::on_mouse_up }),
        "on_mouse_move" => Some(quote! { ::rfgui::ui::on_mouse_move }),
        "on_mouse_enter" => Some(quote! { ::rfgui::ui::on_mouse_enter }),
        "on_mouse_leave" => Some(quote! { ::rfgui::ui::on_mouse_leave }),
        "on_click" => Some(quote! { ::rfgui::ui::on_click }),
        "on_key_down" => Some(quote! { ::rfgui::ui::on_key_down }),
        "on_key_up" => Some(quote! { ::rfgui::ui::on_key_up }),
        "on_focus" if is_text_area_tag(tag) => Some(quote! { ::rfgui::ui::on_text_area_focus }),
        "on_blur" => Some(quote! { ::rfgui::ui::on_blur }),
        "on_focus" => Some(quote! { ::rfgui::ui::on_focus }),
        _ => None,
    }
}

fn wrap_event_expr(
    tag: &Path,
    prop_key: &str,
    expr: &Expr,
    raw: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let Some(converter_fn) = event_handler_converter(tag, prop_key) else {
        return raw;
    };

    if let Expr::Closure(closure) = expr
        && closure.inputs.is_empty()
    {
        return quote! { #converter_fn(::rfgui::ui::no_arg_handler(#raw)) };
    }

    if matches!(expr, Expr::Closure(_))
        && let Some(wrapper_fn) = event_handler_wrapper(tag, prop_key)
    {
        quote! { #wrapper_fn(#raw) }
    } else if matches!(expr, Expr::Closure(_)) {
        quote! { #converter_fn(#raw) }
    } else {
        raw
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
        PropValueExpr::Expr(value) => quote! { #value },
        PropValueExpr::StyleObject(entries) => {
            let style_inserts = entries.iter().map(expand_style_entry);
            quote! {{
                let mut __rsx_style = ::rfgui::Style::new();
                #(#style_inserts)*
                __rsx_style
            }}
        }
        PropValueExpr::Object(entries) => {
            expand_object_value_for_builder(tag, key, entries, builder_ident)
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

fn expand_object_value_for_builder(
    tag: &Path,
    key: &Ident,
    entries: &[ObjectEntry],
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let type_method_ident = format_ident!("__rsx_prop_type_{}", key);
    let assignments = entries
        .iter()
        .map(|entry| expand_object_assignment(tag, entry, quote!(__rsx_object_builder)));
    quote! {{
        ::rfgui::ui::build_typed_prop_for(#builder_ident.#type_method_ident(), |__rsx_object_builder| {
            #(#assignments)*
        })
    }}
}

fn expand_object_assignment(
    tag: &Path,
    entry: &ObjectEntry,
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let key_ident = &entry.key;
    let value = expand_object_value_expr(tag, key_ident, &entry.value, builder_ident.clone());
    quote! {
        let _ = &#builder_ident.#key_ident;
        #builder_ident.#key_ident(#value);
    }
}

fn expand_object_value_expr(
    tag: &Path,
    key: &Ident,
    value: &ObjectValueExpr,
    builder_ident: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    match value {
        ObjectValueExpr::Expr(value) => {
            let raw = quote! { #value };
            wrap_event_expr(tag, &key.to_string(), value, raw)
        }
        ObjectValueExpr::StyleObject(entries) => {
            let style_inserts = entries.iter().map(expand_style_entry);
            quote! {{
                let mut __rsx_style = ::rfgui::Style::new();
                #(#style_inserts)*
                __rsx_style
            }}
        }
        ObjectValueExpr::Object(entries) => {
            let type_method_ident = format_ident!("__rsx_prop_type_{}", key);
            let assignments = entries
                .iter()
                .map(|entry| expand_object_assignment(tag, entry, quote!(__rsx_nested_builder)));
            quote! {{
                ::rfgui::ui::build_typed_prop_for(#builder_ident.#type_method_ident(), |__rsx_nested_builder| {
                    #(#assignments)*
                })
            }}
        }
    }
}

fn expand_style_entry(entry: &StyleEntry) -> proc_macro2::TokenStream {
    let key_ident = &entry.key;
    let key = entry.key.to_string();
    if matches!(&entry.value, StyleValueExpr::Missing) {
        return quote! {
            let _ = |__style_schema: &::rfgui::ui::host::ElementStylePropSchema| {
                let _ = &__style_schema.#key_ident;
            };
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
    let expr_style_tokens =
        |error_message: &str, expand_expr: fn(&Expr) -> proc_macro2::TokenStream| match &entry.value
        {
            StyleValueExpr::Expr(value) => expand_expr(value),
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!(#error_message);
            },
            StyleValueExpr::Missing => unreachable!("missing style value handled above"),
        };
    let style_value_tokens = match key.as_str() {
        "border" => expr_style_tokens("style.border requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| quote! { __rsx_style.set_border(#inner); })
        }),
        "background" | "background_color" => {
            expr_style_tokens("style.background requires an expression value", |value| {
                expand_color_style_value(value, quote!(::rfgui::PropertyId::BackgroundColor))
            })
        }
        "color" => expr_style_tokens("style.color requires an expression value", |value| {
            expand_color_style_value(value, quote!(::rfgui::PropertyId::Color))
        }),
        "font" => expr_style_tokens("style.font requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    __rsx_style.insert(
                        ::rfgui::PropertyId::FontFamily,
                        ::rfgui::ParsedValue::FontFamily(#inner),
                    );
                }
            })
        }),
        "font_size" => expr_style_tokens("style.font_size requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    ::rfgui::insert_style_font_size(
                        &mut __rsx_style,
                        ::rfgui::PropertyId::FontSize,
                        #inner,
                    );
                }
            })
        }),
        "font_weight" => {
            expr_style_tokens("style.font_weight requires an expression value", |value| {
                expand_maybe_none_style_expr(value, |inner| {
                    quote! {
                        ::rfgui::insert_style_font_weight(
                            &mut __rsx_style,
                            ::rfgui::PropertyId::FontWeight,
                            #inner,
                        );
                    }
                })
            })
        }
        "text_wrap" => expr_style_tokens("style.text_wrap requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    ::rfgui::insert_style_text_wrap(
                        &mut __rsx_style,
                        ::rfgui::PropertyId::TextWrap,
                        #inner,
                    );
                }
            })
        }),
        "border_radius" => expr_style_tokens(
            "style.border_radius requires an expression value",
            |value| {
                expand_maybe_none_style_expr(value, |inner| {
                    quote! {
                        __rsx_style.set_border_radius(::rfgui::IntoBorderRadius::into_border_radius(#inner));
                    }
                })
            },
        ),
        "opacity" => expr_style_tokens("style.opacity requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    __rsx_style.insert(
                        ::rfgui::PropertyId::Opacity,
                        ::rfgui::ParsedValue::Opacity(::rfgui::Opacity::new((#inner) as f32)),
                    );
                }
            })
        }),
        "box_shadow" => {
            expr_style_tokens("style.box_shadow requires an expression value", |value| {
                expand_maybe_none_style_expr(value, |inner| {
                    quote! {
                        __rsx_style.insert(
                            ::rfgui::PropertyId::BoxShadow,
                            ::rfgui::ParsedValue::BoxShadow(#inner),
                        );
                    }
                })
            })
        }
        "transition" => {
            expr_style_tokens("style.transition requires an expression value", |value| {
                expand_maybe_none_style_expr(value, |inner| {
                    quote! {
                        __rsx_style.insert(
                            ::rfgui::PropertyId::Transition,
                            ::rfgui::ParsedValue::Transition((#inner).into()),
                        );
                    }
                })
            })
        }
        "padding" => expr_style_tokens("style.padding requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| quote! { __rsx_style.set_padding(#inner); })
        }),
        "position" => expr_style_tokens("style.position requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    __rsx_style.insert(
                        ::rfgui::PropertyId::Position,
                        ::rfgui::ParsedValue::Position(#inner),
                    );
                }
            })
        }),
        "width" => expr_style_tokens("style.width requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    ::rfgui::insert_style_length(
                        &mut __rsx_style,
                        ::rfgui::PropertyId::Width,
                        #inner,
                    );
                }
            })
        }),
        "min_width" => expr_style_tokens("style.min_width requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    ::rfgui::insert_style_length(
                        &mut __rsx_style,
                        ::rfgui::PropertyId::MinWidth,
                        #inner,
                    );
                }
            })
        }),
        "max_width" => expr_style_tokens("style.max_width requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    ::rfgui::insert_style_length(
                        &mut __rsx_style,
                        ::rfgui::PropertyId::MaxWidth,
                        #inner,
                    );
                }
            })
        }),
        "height" => expr_style_tokens("style.height requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    ::rfgui::insert_style_length(
                        &mut __rsx_style,
                        ::rfgui::PropertyId::Height,
                        #inner,
                    );
                }
            })
        }),
        "min_height" => {
            expr_style_tokens("style.min_height requires an expression value", |value| {
                expand_maybe_none_style_expr(value, |inner| {
                    quote! {
                        ::rfgui::insert_style_length(
                            &mut __rsx_style,
                            ::rfgui::PropertyId::MinHeight,
                            #inner,
                        );
                    }
                })
            })
        }
        "max_height" => {
            expr_style_tokens("style.max_height requires an expression value", |value| {
                expand_maybe_none_style_expr(value, |inner| {
                    quote! {
                        ::rfgui::insert_style_length(
                            &mut __rsx_style,
                            ::rfgui::PropertyId::MaxHeight,
                            #inner,
                        );
                    }
                })
            })
        }
        "layout" => expr_style_tokens("style.layout requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    __rsx_style.insert(
                        ::rfgui::PropertyId::Layout,
                        ::rfgui::ParsedValue::Layout((#inner).into()),
                    );
                }
            })
        }),
        "cross_size" => {
            expr_style_tokens("style.cross_size requires an expression value", |value| {
                expand_maybe_none_style_expr(value, |inner| {
                    quote! {
                        __rsx_style.insert(
                            ::rfgui::PropertyId::CrossSize,
                            ::rfgui::ParsedValue::CrossSize(#inner),
                        );
                    }
                })
            })
        }
        "align" => expr_style_tokens("style.align requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    __rsx_style.insert(
                        ::rfgui::PropertyId::Align,
                        ::rfgui::ParsedValue::Align(#inner),
                    );
                }
            })
        }),
        "flex" => expr_style_tokens("style.flex requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    ::rfgui::insert_style_flex(
                        &mut __rsx_style,
                        ::rfgui::PropertyId::Flex,
                        #inner,
                    );
                }
            })
        }),
        "gap" => expr_style_tokens("style.gap requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    ::rfgui::insert_style_length(
                        &mut __rsx_style,
                        ::rfgui::PropertyId::Gap,
                        #inner,
                    );
                }
            })
        }),
        "scroll_direction" => expr_style_tokens(
            "style.scroll_direction requires an expression value",
            |value| {
                expand_maybe_none_style_expr(value, |inner| {
                    quote! {
                        __rsx_style.insert(
                            ::rfgui::PropertyId::ScrollDirection,
                            ::rfgui::ParsedValue::ScrollDirection(#inner),
                        );
                    }
                })
            },
        ),
        "cursor" => expr_style_tokens("style.cursor requires an expression value", |value| {
            expand_maybe_none_style_expr(value, |inner| {
                quote! {
                    __rsx_style.insert(
                        ::rfgui::PropertyId::Cursor,
                        ::rfgui::ParsedValue::Cursor(#inner),
                    );
                }
            })
        }),
        "hover" => match &entry.value {
            StyleValueExpr::StyleObject(entries) => {
                let hover_inserts = entries.iter().map(expand_style_entry);
                quote! {
                    let mut __rsx_hover_style = ::rfgui::Style::new();
                    {
                        let __rsx_style = &mut __rsx_hover_style;
                        #(#hover_inserts)*
                    }
                    __rsx_style.set_hover(__rsx_hover_style);
                }
            }
            StyleValueExpr::Expr(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.hover requires object syntax, e.g. hover: {{ background: \"#fff\" }}");
            },
            StyleValueExpr::Missing => quote_spanned! {entry.key.span()=>
                compile_error!("style.hover is incomplete; expected `:` and a value");
            },
        },
        _ => quote_spanned! {entry.key.span()=>
            compile_error!("unsupported style key");
        },
    };

    quote! {
        let _ = |__style_schema: &::rfgui::ui::host::ElementStylePropSchema| {
            let _ = &__style_schema.#key_ident;
        };
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

fn expand_color_style_value(
    value: &Expr,
    property: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    expand_maybe_none_style_expr(value, |inner| {
        quote! {
            __rsx_style.insert(#property, ::rfgui::style_color_value(#inner));
        }
    })
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
            builder_prop_type_methods.push(quote! {
                pub fn #type_method_ident(&self) -> ::core::marker::PhantomData<#inner_ty> {
                    ::core::marker::PhantomData
                }
            });
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
            builder_prop_type_methods.push(quote! {
                pub fn #type_method_ident(&self) -> ::core::marker::PhantomData<#props_field_ty> {
                    ::core::marker::PhantomData
                }
            });
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
}
