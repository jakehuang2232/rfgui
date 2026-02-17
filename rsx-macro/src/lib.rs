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
    let nodes = match syn::parse::<MultipleNodes>(input) {
        Ok(m) => m.nodes,
        Err(err) => return err.to_compile_error().into(),
    };

    if nodes.len() == 1 {
        expand_node(&nodes[0]).into()
    } else {
        let children = nodes.iter().map(expand_node);
        quote! {
            ::rust_gui::ui::RsxNode::fragment(vec![
                #(#children),*
            ])
        }
        .into()
    }
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
                    PropValueExpr::Expr(content.parse()?)
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
            return Err(syn::Error::new(close_tag.span(), "closing tag does not match"));
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

fn parse_style_entries(input: ParseStream) -> Result<Vec<StyleEntry>> {
    let mut entries = Vec::new();
    while !input.is_empty() {
        let style_key: Ident = input.parse()?;
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
    let child_statements = element.children.iter().map(|c| {
        let child_expr = expand_node(c);
        quote! {
            __rsx_children.push(#child_expr);
        }
    });

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
    let value_tokens = match &prop.value {
        PropValueExpr::Expr(value) => {
            quote! {
                __rsx_props.push(#key, ::rust_gui::ui::IntoPropValue::into_prop_value(#value));
            }
        }
        PropValueExpr::StyleObject(entries) => {
            let style_inserts = entries.iter().map(expand_style_entry);
            quote! {
                let mut __rsx_style = ::rust_gui::Style::new();
                #(#style_inserts)*
                __rsx_props.push(#key, ::rust_gui::ui::IntoPropValue::into_prop_value(__rsx_style));
            }
        }
    };
    quote! {
        let _ = |__schema: &<#tag as ::rust_gui::ui::RsxPropSchema>::PropsSchema| {
            let _ = &__schema.#key_ident;
        };
        #value_tokens
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
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::BackgroundColor,
                    ::rust_gui::ParsedValue::Color(
                        ::rust_gui::IntoColor::<::rust_gui::Color>::into_color(#value)
                    ),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.background requires an expression value");
            },
        },
        "border_radius" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.set_border_radius(::rust_gui::IntoBorderRadius::into_border_radius(#value));
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.border_radius requires an expression value");
            },
        },
        "opacity" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::Opacity,
                    ::rust_gui::ParsedValue::Opacity(::rust_gui::Opacity::new((#value) as f32)),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.opacity requires an expression value");
            },
        },
        "transition" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::Transition,
                    ::rust_gui::ParsedValue::Transition((#value).into()),
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
        "width" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::Width,
                    ::rust_gui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.width requires an expression value");
            },
        },
        "height" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::Height,
                    ::rust_gui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.height requires an expression value");
            },
        },
        "display" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::Display,
                    ::rust_gui::ParsedValue::Display(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.display requires an expression value");
            },
        },
        "flow_direction" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::FlowDirection,
                    ::rust_gui::ParsedValue::FlowDirection(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.flow_direction requires an expression value");
            },
        },
        "flow_wrap" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::FlowWrap,
                    ::rust_gui::ParsedValue::FlowWrap(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.flow_wrap requires an expression value");
            },
        },
        "justify_content" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::JustifyContent,
                    ::rust_gui::ParsedValue::JustifyContent(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.justify_content requires an expression value");
            },
        },
        "align_items" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::AlignItems,
                    ::rust_gui::ParsedValue::AlignItems(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.align_items requires an expression value");
            },
        },
        "gap" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::Gap,
                    ::rust_gui::ParsedValue::Length(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.gap requires an expression value");
            },
        },
        "scroll_direction" => match &entry.value {
            StyleValueExpr::Expr(value) => quote! {
                __rsx_style.insert(
                    ::rust_gui::PropertyId::ScrollDirection,
                    ::rust_gui::ParsedValue::ScrollDirection(#value),
                );
            },
            StyleValueExpr::StyleObject(_) => quote_spanned! {entry.key.span()=>
                compile_error!("style.scroll_direction requires an expression value");
            },
        },
        "hover" => match &entry.value {
            StyleValueExpr::StyleObject(entries) => {
                let hover_inserts = entries.iter().map(expand_style_entry);
                quote! {
                    let mut __rsx_hover_style = ::rust_gui::Style::new();
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
        let _ = |__style_schema: &::rust_gui::ui::host::ElementStylePropSchema| {
            let _ = &__style_schema.#key_ident;
        };
        #style_value_tokens
    }
}

fn expand_node(child: &Child) -> proc_macro2::TokenStream {
    match child {
        Child::Element(element) => expand_element(element),
        Child::TextLiteral(text) => {
            quote! {
                ::rust_gui::ui::RsxNode::text(#text)
            }
        }
        Child::TextRaw(text) => {
            quote! {
                ::rust_gui::ui::RsxNode::text(#text)
            }
        }
        Child::Expr(expr) => {
            quote! {
                ::rust_gui::ui::IntoRsxNode::into_rsx_node(#expr)
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
        return syn::Error::new(input_fn.sig.generics.span(), "#[component] does not support generics yet")
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
            return syn::Error::new(arg.span(), "#[component] does not support method receivers")
                .to_compile_error();
        };

        let Pat::Ident(PatIdent { ident, .. }) = pat_ty.pat.as_ref() else {
            return syn::Error::new(pat_ty.pat.span(), "#[component] parameters must be simple identifiers")
                .to_compile_error();
        };

        let field_ident = ident.clone();
        let ty = pat_ty.ty.as_ref().clone();

        if field_ident == "children" {
            let is_children_ty = is_vec_rsx_node(&ty);
            if !is_children_ty {
                return syn::Error::new(ty.span(), "children type must be Vec<RsxNode>")
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
