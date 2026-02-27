use proc_macro::TokenStream;
use proc_macro2::{Delimiter, Span, TokenTree};
use quote::{ToTokens, format_ident, quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Expr, Fields, FnArg, Ident, ItemFn, ItemStruct, Lit, LitStr, Pat, PatIdent, Path, Result,
    ReturnType, Token, Type, TypePath, braced, parse_quote,
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
}

#[derive(Clone)]
enum StyleValueExpr {
    Expr(Expr),
    StyleObject(Vec<StyleEntry>),
}

#[derive(Clone)]
struct StyleEntry {
    key: Ident,
    value: StyleValueExpr,
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
            input.parse::<Token![=]>()?;
            let value: PropValueExpr = if input.peek(syn::token::Brace) {
                let content;
                braced!(content in input);
                if key == "style" && content.peek(syn::token::Brace) {
                    let style_content;
                    braced!(style_content in content);
                    let entries = parse_style_entries(&style_content)?;
                    if !content.is_empty() {
                        return Err(syn::Error::new(content.span(), "style object syntax error"));
                    }
                    PropValueExpr::StyleObject(entries)
                } else {
                    let expr_tokens: proc_macro2::TokenStream = content.parse()?;
                    match syn::parse2::<Expr>(expr_tokens.clone()) {
                        Ok(expr) => PropValueExpr::Expr(expr),
                        Err(parse_err) => {
                            if let Some(object_span) = rsx_object_assignment_span(&expr_tokens) {
                                let key_name = key.to_string();
                                return Err(syn::Error::new(
                                    object_span,
                                    format!(
                                        "invalid RSX object syntax on prop `{}`: use a Rust expression (for example `{0}=Some(Type {{ ... }})`). RSX object syntax `{{{{ key: value }}}}` is only for `style`.",
                                        key_name
                                    ),
                                ));
                            }
                            if let Some(assign_span) = prop_assignment_like_span(&expr_tokens) {
                                return Err(syn::Error::new(
                                    assign_span,
                                    format!(
                                        "syntax error inside prop `{}`: `{{...}}` must be a valid Rust expression. It looks like field assignment (`name=...`). Use a Rust value such as `Some(Type {{ name: value }})`.",
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
                            return Err(err);
                        }
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
            return Err(syn::Error::new(
                close_tag.span(),
                "closing tag does not match",
            ));
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

fn rsx_object_assignment_span(tokens: &proc_macro2::TokenStream) -> Option<Span> {
    let mut iter = tokens.clone().into_iter();
    let Some(TokenTree::Group(group)) = iter.next() else {
        return None;
    };
    if group.delimiter() != Delimiter::Brace || iter.next().is_some() {
        return None;
    }
    let inner = group.stream().to_string();
    if inner.contains('=') && inner.contains(',') {
        return Some(group.span());
    }
    None
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
    let child_statements = element.children.iter().map(|c| {
        let child_expr = expand_node(c);
        quote! {
            __rsx_children.push(#child_expr);
        }
    });

    if should_fill_default_props(tag) {
        let Some(direct_props_type) = direct_props_type_tokens(tag) else {
            return syn::Error::new(tag.span(), "unsupported default-props host component tag")
                .to_compile_error();
        };
        let prop_schema_checks = element
            .props
            .iter()
            .filter(|p| p.key != "key")
            .map(expand_direct_prop_schema_check);
        let prop_assignments = element
            .props
            .iter()
            .filter(|p| p.key != "key")
            .map(expand_direct_prop_assignment);
        let children_assignment = if has_children {
            quote! { children: __rsx_children, }
        } else {
            quote! {}
        };
        return quote! {
            {
                let _ = ::core::marker::PhantomData::<#close_tag>;
                let mut __rsx_children = Vec::<::rfgui::ui::RsxNode>::new();
                #(#child_statements)*
                type __RsxComponentProps = #direct_props_type;
                #(#prop_schema_checks)*
                let __rsx_props: __RsxComponentProps = __RsxComponentProps {
                    #(#prop_assignments)*
                    #children_assignment
                    ..<__RsxComponentProps as ::rfgui::ui::OptionalDefault>::optional_default()
                };
                <#tag as ::rfgui::ui::RsxComponent<__RsxComponentProps>>::render(__rsx_props)
            }
        };
    }

    let prop_schema_checks = element
        .props
        .iter()
        .filter(|p| p.key != "key")
        .map(expand_builder_prop_schema_check);
    let builder_assignments = element
        .props
        .iter()
        .filter(|p| p.key != "key")
        .map(expand_builder_assignment);
    let component_key = component_key_tokens(element);
    let children_assignment = if has_children {
        quote! { __rsx_props_builder.children = ::core::option::Option::Some(__rsx_children); }
    } else {
        quote! {}
    };
    let children_schema_check = if has_children {
        quote! {
            let _ = &__rsx_props_builder.children;
        }
    } else {
        quote! {}
    };
    quote! {
        {
            let _ = ::core::marker::PhantomData::<#close_tag>;
            let mut __rsx_children = Vec::<::rfgui::ui::RsxNode>::new();
            #(#child_statements)*
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
            fn __rsx_render_props<__RsxComponentProps>(props: __RsxComponentProps) -> ::rfgui::ui::RsxNode
            where
                #tag: ::rfgui::ui::RsxComponent<__RsxComponentProps>,
            {
                <#tag as ::rfgui::ui::RsxComponent<__RsxComponentProps>>::render(props)
            }
            let mut __rsx_props_builder = __rsx_builder_for_props::<_>();
            #(#prop_schema_checks)*
            #children_schema_check
            #(#builder_assignments)*
            #children_assignment
            let __rsx_props = __rsx_build_props(__rsx_props_builder)
                .expect(concat!("rsx build error on <", stringify!(#tag), ">"));
            ::rfgui::ui::with_component_key(#component_key, || {
                ::rfgui::ui::render_component::<#tag, _>(|| {
                    __rsx_render_props(__rsx_props)
                })
            })
        }
    }
}

fn component_key_tokens(element: &ElementNode) -> proc_macro2::TokenStream {
    let Some(prop) = element.props.iter().find(|p| p.key == "key") else {
        return quote! { ::core::option::Option::None };
    };
    match &prop.value {
        PropValueExpr::Expr(expr) => {
            quote! { ::core::option::Option::Some(::rfgui::ui::component_key_token(&(#expr))) }
        }
        PropValueExpr::StyleObject(_) => quote_spanned! {prop.key.span()=>
            compile_error!("`key` must be a Rust expression, not a style object")
        },
    }
}

fn should_fill_default_props(path: &Path) -> bool {
    matches!(
        tag_name(path).as_deref(),
        Some("Element" | "Text" | "TextArea")
    )
}

fn direct_props_type_tokens(path: &Path) -> Option<proc_macro2::TokenStream> {
    match tag_name(path).as_deref() {
        Some("Element") => Some(quote! { ::rfgui::ui::host::ElementPropSchema }),
        Some("Text") => Some(quote! { ::rfgui::ui::host::TextPropSchema }),
        Some("TextArea") => Some(quote! { ::rfgui::ui::host::TextAreaPropSchema }),
        _ => None,
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
        builder_fields.push(quote! {
            pub #field_ident: ::core::option::Option<#field_ty>,
        });
        builder_default_fields.push(quote! {
            #field_ident: ::core::option::Option::None,
        });
        if let Some(inner_ty) = option_inner_type(field_ty) {
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
                        V: ::core::convert::Into<::core::option::Option<#inner_ty>>,
                    {
                        self.#field_ident = ::core::option::Option::Some(value.into());
                    }
                });
            }
            build_fields.push(quote! {
                #field_ident: builder.#field_ident.unwrap_or(::core::option::Option::None),
            });
        } else if is_children_field(field_ident, field_ty) {
            all_optional = false;
            builder_setters.push(quote! {
                pub fn #field_ident(&mut self, value: #field_ty) {
                    self.#field_ident = ::core::option::Option::Some(value);
                }
            });
            build_fields.push(quote! {
                #field_ident: builder.#field_ident.unwrap_or_default(),
            });
        } else {
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

fn expand_direct_prop_assignment(prop: &Prop) -> proc_macro2::TokenStream {
    let key_ident = &prop.key;
    let value = expand_prop_value_expr(&prop.value);
    quote! {
        #key_ident: (#value).into(),
    }
}

fn expand_builder_assignment(prop: &Prop) -> proc_macro2::TokenStream {
    let key_ident = &prop.key;
    let value = expand_prop_value_expr(&prop.value);
    quote! {
        __rsx_props_builder.#key_ident(#value);
    }
}

fn expand_direct_prop_schema_check(prop: &Prop) -> proc_macro2::TokenStream {
    let key_ident = &prop.key;
    quote! {
        let _ = |__schema: &__RsxComponentProps| {
            let _ = &__schema.#key_ident;
        };
    }
}

fn expand_builder_prop_schema_check(prop: &Prop) -> proc_macro2::TokenStream {
    let key_ident = &prop.key;
    quote! {
        let _ = &__rsx_props_builder.#key_ident;
    }
}

fn expand_prop_value_expr(value: &PropValueExpr) -> proc_macro2::TokenStream {
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
    }
}

fn expand_style_entry(entry: &StyleEntry) -> proc_macro2::TokenStream {
    let key_ident = &entry.key;
    let key = entry.key.to_string();
    let style_value_tokens = match key.as_str() {
        "border" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! { __rsx_style.set_border(#value); },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.border requires an expression value");
            },
        },
        "background" | "background_color" => match &entry.value {
            StyleValueExpr::Expr(value) => expand_color_style_value(
                value,
                quote!(::rfgui::PropertyId::BackgroundColor),
            ),
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.background requires an expression value");
            },
        },
        "color" => match &entry.value {
            StyleValueExpr::Expr(value) => {
                expand_color_style_value(value, quote!(::rfgui::PropertyId::Color))
            }
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.color requires an expression value");
            },
        },
        "font" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::FontFamily,
                    ::rfgui::ParsedValue::FontFamily(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.font requires an expression value");
            },
        },
        "font_size" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::FontSize,
                    ::rfgui::ParsedValue::FontSize(
                        ::rfgui::IntoFontSize::into_font_size(#value)
                    ),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.font_size requires an expression value");
            },
        },
        "font_weight" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::FontWeight,
                    ::rfgui::ParsedValue::FontWeight(
                        ::rfgui::IntoFontWeight::into_font_weight(#value)
                    ),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.font_weight requires an expression value");
            },
        },
        "border_radius" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.set_border_radius(::rfgui::IntoBorderRadius::into_border_radius(#value));
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.border_radius requires an expression value");
            },
        },
        "opacity" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::Opacity,
                    ::rfgui::ParsedValue::Opacity(::rfgui::Opacity::new((#value) as f32)),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.opacity requires an expression value");
            },
        },
        "box_shadow" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::BoxShadow,
                    ::rfgui::ParsedValue::BoxShadow(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.box_shadow requires an expression value");
            },
        },
        "transition" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::Transition,
                    ::rfgui::ParsedValue::Transition((#value).into()),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.transition requires an expression value");
            },
        },
        "padding" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! { __rsx_style.set_padding(#value); },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.padding requires an expression value");
            },
        },
        "position" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::Position,
                    ::rfgui::ParsedValue::Position(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.position requires an expression value");
            },
        },
        "width" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::Width,
                    ::rfgui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.width requires an expression value");
            },
        },
        "min_width" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::MinWidth,
                    ::rfgui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.min_width requires an expression value");
            },
        },
        "max_width" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::MaxWidth,
                    ::rfgui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.max_width requires an expression value");
            },
        },
        "height" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::Height,
                    ::rfgui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.height requires an expression value");
            },
        },
        "min_height" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::MinHeight,
                    ::rfgui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.min_height requires an expression value");
            },
        },
        "max_height" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::MaxHeight,
                    ::rfgui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.max_height requires an expression value");
            },
        },
        "display" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::Display,
                    ::rfgui::ParsedValue::Display(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.display requires an expression value");
            },
        },
        "align_items" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::AlignItems,
                    ::rfgui::ParsedValue::AlignItems(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.align_items requires an expression value");
            },
        },
        "gap" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::Gap,
                    ::rfgui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.gap requires an expression value");
            },
        },
        "scroll_direction" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::ScrollDirection,
                    ::rfgui::ParsedValue::ScrollDirection(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.scroll_direction requires an expression value");
            },
        },
        "cursor" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rfgui::PropertyId::Cursor,
                    ::rfgui::ParsedValue::Cursor(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.cursor requires an expression value");
            },
        },
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

fn expand_color_style_value(
    value: &Expr,
    property: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote! {
        {
            let __rsx_color = ::rfgui::IntoColor::<::rfgui::Color>::into_color(#value);
            __rsx_style.insert(
                #property,
                ::rfgui::ParsedValue::Color(__rsx_color),
            );
        }
    }
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

fn tag_name(path: &Path) -> Option<String> {
    path.segments.last().map(|seg| seg.ident.to_string())
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
    let mut builder_default_fields = Vec::new();
    let mut build_fields = Vec::new();
    let mut optional_default_fields = Vec::new();
    let mut all_optional = true;
    let mut helper_args = Punctuated::<FnArg, Token![,]>::new();
    let mut helper_call_args = Vec::new();

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
            parse_quote!(Vec<::rfgui::ui::RsxNode>)
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
                        V: ::core::convert::Into<::core::option::Option<#inner_ty>>,
                    {
                        self.#field_ident = ::core::option::Option::Some(value.into());
                    }
                });
            }
            build_fields.push(quote! {
                #field_ident: builder.#field_ident.unwrap_or(::core::option::Option::None),
            });
        } else if is_children_field(&field_ident, &props_field_ty) {
            all_optional = false;
            builder_setters.push(quote! {
                pub fn #field_ident(&mut self, value: #props_field_ty) {
                    self.#field_ident = ::core::option::Option::Some(value);
                }
            });
            build_fields.push(quote! {
                #field_ident: builder.#field_ident.unwrap_or_default(),
            });
        } else {
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
            fn render(props: #props_name #ty_generics) -> ::rfgui::ui::RsxNode {
                ::rfgui::ui::render_component::<Self, _>(|| {
                    #helper_name(#(#helper_call_args),*)
                })
            }
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
