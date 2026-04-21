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
        ::rfgui::ui::rsx_scope(|| {
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
    // Two accepted forms:
    //   1. `#[component] fn Foo(...) -> RsxNode { ... }`
    //      — generates the whole component (struct + RsxComponent + RsxTag +
    //        vtable + ComponentTag).
    //   2. `#[component] impl RsxTag for Foo { ... }`
    //      — user authored `struct Foo` + `impl RsxComponent<FooProps>`
    //        themselves. We augment the RsxTag impl with the vtable override
    //        and emit the shims + `impl ComponentTag`. Enables lazy render
    //        for hand-written components without rewriting them as a fn.
    let item2: proc_macro2::TokenStream = item.clone().into();
    if let Ok(input_impl) = syn::parse2::<syn::ItemImpl>(item2) {
        return expand_component_impl(input_impl).into();
    }
    let input_fn = syn::parse_macro_input!(item as ItemFn);
    expand_component(input_fn).into()
}

#[proc_macro_attribute]
pub fn props(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[props] takes no arguments (軌 1 #11: host= / custom_update= removed — \
             dispatch now lives on the host via ElementTrait::apply_prop)",
        )
        .to_compile_error()
        .into();
    }
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
    Object(Vec<ObjectEntry>),
    Missing,
    Invalid(proc_macro2::TokenStream),
}

#[derive(Clone)]
struct ObjectEntry {
    key: Ident,
    value: ObjectValueExpr,
}

#[derive(Clone)]
enum ObjectValueExpr {
    Expr(Expr),
    Object(Vec<ObjectEntry>),
    Missing,
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
    let object_tokens: proc_macro2::TokenStream = input.fork().parse()?;
    // Double-brace (`prop={{...}}`) is the committed-object signal: the inner
    // brace group was preserved after the outer `braced!` strip. Once
    // committed, enable recovery so partial entries (e.g. `w` with no `:`)
    // survive parse and rust-analyzer can offer field completions.
    if let Some(inner_tokens) = unwrap_single_brace_group(&object_tokens)
        && let Ok(entries) = parse_object_entries_from_tokens(inner_tokens, true)
    {
        let _: proc_macro2::TokenStream = input.parse()?;
        return Ok(PropValueExpr::Object(entries));
    }

    // Single-brace object (`prop={foo: bar}`) — probe strict to avoid
    // misclassifying plain ident exprs like `prop={ident}` as `{ident: Missing}`.
    if let Ok(entries) = parse_object_entries_from_tokens(object_tokens.clone(), false) {
        let _: proc_macro2::TokenStream = input.parse()?;
        return Ok(PropValueExpr::Object(entries));
    }

    if input.peek(syn::token::Brace) {
        let fork = input.fork();
        let nested;
        braced!(nested in fork);
        let nested_tokens: proc_macro2::TokenStream = nested.parse()?;
        if let Ok(_entries) = parse_object_entries_from_tokens(nested_tokens, false)
            && fork.is_empty()
        {
            let object_content;
            braced!(object_content in input);
            let object_tokens: proc_macro2::TokenStream = object_content.parse()?;
            let entries = parse_object_entries_from_tokens(object_tokens, true)?;
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

fn parse_object_entries(input: ParseStream, recover: bool) -> Result<Vec<ObjectEntry>> {
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
        if !input.peek(Token![:]) {
            if recover && can_recover_incomplete_object_entry(input) {
                entries.push(ObjectEntry {
                    key,
                    value: ObjectValueExpr::Missing,
                });
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }
            return Err(syn::Error::new(
                input.span(),
                format!("expected `:` after key `{}`", key),
            ));
        }
        input.parse::<Token![:]>()?;
        let value = if input.peek(syn::token::Brace) {
            let nested;
            braced!(nested in input);
            let nested_tokens: proc_macro2::TokenStream = nested.parse()?;
            ObjectValueExpr::Object(parse_object_entries_from_tokens(nested_tokens, recover)?)
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

fn can_recover_incomplete_object_entry(input: ParseStream) -> bool {
    input.is_empty() || input.peek(Token![,])
}

fn parse_object_entries_from_tokens(
    tokens: proc_macro2::TokenStream,
    recover: bool,
) -> Result<Vec<ObjectEntry>> {
    struct ObjectEntries {
        entries: Vec<ObjectEntry>,
    }
    struct ObjectEntriesRecover {
        entries: Vec<ObjectEntry>,
    }

    impl Parse for ObjectEntries {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                entries: parse_object_entries(input, false)?,
            })
        }
    }
    impl Parse for ObjectEntriesRecover {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                entries: parse_object_entries(input, true)?,
            })
        }
    }

    if recover {
        syn::parse2::<ObjectEntriesRecover>(tokens).map(|p| p.entries)
    } else {
        syn::parse2::<ObjectEntries>(tokens).map(|p| p.entries)
    }
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

fn component_key_tokens(element: &ElementNode) -> proc_macro2::TokenStream {
    let Some(prop) = element.props.iter().find(|p| p.key == "key") else {
        return quote! { ::core::option::Option::None };
    };
    match &prop.value {
        PropValueExpr::Expr(expr) => {
            quote! { ::core::option::Option::Some(::rfgui::ui::classify_component_key(&(#expr))) }
        }
        PropValueExpr::Macro(_) | PropValueExpr::Object(_) => {
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
    let init_ident = format_ident!("__{}Init", struct_ident);
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

    let struct_name_str = struct_ident.to_string();
    let mut default_fields = Vec::new();
    let mut init_fields = Vec::new();
    let mut init_default_fields = Vec::new();
    let mut from_init_fields = Vec::new();
    let mut all_optional = true;
    for field in fields {
        let field_ident = match &field.ident {
            Some(ident) => ident,
            None => {
                return syn::Error::new(field.span(), "#[prop] field must be named")
                    .to_compile_error();
            }
        };
        let field_ty = &field.ty;
        if field_ident == "children" {
            return syn::Error::new(
                field.span(),
                "`children` is not a props field; declare it only as a #[component] function parameter",
            )
            .to_compile_error();
        }
        let (init_inner, is_already_option) = match option_inner_type(field_ty) {
            Some(inner) => (inner.clone(), true),
            None => (field_ty.clone(), false),
        };
        init_fields.push(quote! {
            pub #field_ident: ::core::option::Option<#init_inner>,
        });
        init_default_fields.push(quote! {
            #field_ident: ::core::option::Option::None,
        });
        if is_already_option {
            default_fields.push(quote! {
                #field_ident: ::core::option::Option::None,
            });
            from_init_fields.push(quote! {
                #field_ident: __init.#field_ident,
            });
        } else {
            all_optional = false;
            let field_name = field_ident.to_string();
            from_init_fields.push(quote! {
                #field_ident: __init.#field_ident.expect(concat!(
                    "missing required prop `",
                    #field_name,
                    "` on <",
                    #struct_name_str,
                    ">"
                )),
            });
        }
    }

    let optional_default_impl = if all_optional {
        quote! {
            impl #impl_generics ::core::default::Default for #struct_ident #ty_generics #where_clause {
                fn default() -> Self {
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
        pub struct #init_ident #generics {
            #(#init_fields)*
        }

        impl #impl_generics ::core::default::Default for #init_ident #ty_generics #where_clause {
            fn default() -> Self {
                Self {
                    #(#init_default_fields)*
                }
            }
        }

        impl #impl_generics ::core::convert::From<#init_ident #ty_generics> for #struct_ident #ty_generics #where_clause {
            fn from(__init: #init_ident #ty_generics) -> Self {
                Self {
                    #(#from_init_fields)*
                }
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

/// Closure-valued props route through `From<F>` / `From<NoArgHandler<F>>`
/// so the concrete handler type is selected directly (single matching impl,
/// no ambiguity). The macro stays ignorant of both event names and the `on_*`
/// naming convention — handler-ness falls out of the field's declared type
/// via the `__RsxHandlerField` trait impl. Non-handler props receiving a
/// closure will fail to compile at the helper bound, which is the correct
/// signal.
fn expand_event_closure_assignment(
    key: &Ident,
    expr: &Expr,
    parent_path: &proc_macro2::TokenStream,
) -> Option<proc_macro2::TokenStream> {
    let Expr::Closure(closure) = expr else {
        return None;
    };
    let helper = if closure.inputs.is_empty() {
        quote!(::rfgui::ui::__rsx_ev_no_arg_to_handler)
    } else {
        quote!(::rfgui::ui::__rsx_ev_to_handler)
    };
    Some(quote_spanned! {key.span()=>
        #parent_path.#key = ::core::option::Option::Some(
            #helper(&#parent_path.#key, #expr)
        );
    })
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

/// Key used to match an rsx closing tag against its opening tag. Compares
/// by path segment idents only, stripping `PathArguments` (generics,
/// parenthesized args). Allows the React-style close form:
///
/// ```ignore
/// <Provider::<Ctx> value={v}>
///     ...
/// </Provider>            // generics may be omitted on close
/// ```
///
/// `<Foo>` still rejects `</Bar>` (idents differ), and `<mod::Foo>`
/// rejects `</Foo>` (segment paths differ).
fn path_key(path: &Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// True if the path carries any generic / parenthesized arguments on any
/// segment. Used to decide whether a close tag needs generics-aware
/// PhantomData emission or whether it's a bare-ident form that must
/// borrow generics from the open tag.
fn close_tag_has_args(path: &Path) -> bool {
    path.segments
        .iter()
        .any(|seg| !matches!(seg.arguments, syn::PathArguments::None))
}

fn expand_component_impl(mut input_impl: syn::ItemImpl) -> proc_macro2::TokenStream {
    // Validate that this is `impl <...> RsxTag for T`.
    let trait_ok = input_impl
        .trait_
        .as_ref()
        .map(|(_, path, _)| {
            let last = path.segments.last().map(|s| s.ident.to_string());
            last.as_deref() == Some("RsxTag")
        })
        .unwrap_or(false);
    if !trait_ok {
        return syn::Error::new(
            input_impl.impl_token.span,
            "#[component] on an impl block only supports `impl RsxTag for T` \
             (use `#[component] fn Name(...)` for the fn-style authoring form)",
        )
        .to_compile_error();
    }

    // Locate `type StrictProps = <X>;` so we know what concrete type the
    // vtable shims cast into.
    let mut strict_props_ty: Option<syn::Type> = None;
    for item in &input_impl.items {
        if let syn::ImplItem::Type(ty_item) = item
            && ty_item.ident == "StrictProps"
        {
            strict_props_ty = Some(ty_item.ty.clone());
            break;
        }
    }
    let Some(strict_props_ty) = strict_props_ty else {
        return syn::Error::new(
            input_impl.impl_token.span,
            "#[component] on `impl RsxTag` requires `type StrictProps = <PropsType>;` \
             to know which concrete type to box",
        )
        .to_compile_error();
    };

    // Self type that the impl is for, plus generics for re-emission on the
    // sibling `impl $Self { shims }` / `impl ComponentTag for $Self` blocks.
    let self_ty = input_impl.self_ty.clone();
    let generics = input_impl.generics.clone();
    let (impl_generics, _ty_generics, where_clause) = generics.split_for_impl();

    // Warn / error if `fn component_vtable` is already present — we inject
    // our own below, and duplicate-method would conflict.
    let mut has_vtable_fn = false;
    for item in &input_impl.items {
        if let syn::ImplItem::Fn(f) = item
            && f.sig.ident == "component_vtable"
        {
            has_vtable_fn = true;
            break;
        }
    }
    if !has_vtable_fn {
        let vtable_fn: syn::ImplItemFn = parse_quote! {
            fn component_vtable() -> ::core::option::Option<&'static ::rfgui::ui::ComponentVTable> {
                ::core::option::Option::Some(
                    <Self as ::rfgui::ui::ComponentTag>::VTABLE,
                )
            }
        };
        input_impl.items.push(syn::ImplItem::Fn(vtable_fn));
    }

    // Short type-name literal for `ComponentVTable::type_name`. Uses the
    // token stream of `self_ty` with whitespace stripped — good enough for
    // debug output; matches existing behaviour of `stringify!(#comp_name)`
    // in the fn-form expansion.
    let type_name_str = quote!(#self_ty).to_string().replace(' ', "");

    quote! {
        #input_impl

        // P2/P5: compile-time type-erased dispatch shims. Mono per T.
        #[allow(non_snake_case, dead_code)]
        impl #impl_generics #self_ty #where_clause {
            #[doc(hidden)]
            unsafe fn __rsx_vtable_render_shim(
                props: ::core::ptr::NonNull<()>,
                children: ::std::vec::Vec<::rfgui::ui::RsxNode>,
            ) -> ::rfgui::ui::RsxNode {
                let boxed: ::std::boxed::Box<#strict_props_ty> = unsafe {
                    ::std::boxed::Box::from_raw(props.as_ptr().cast())
                };
                <#self_ty as ::rfgui::ui::RsxComponent<#strict_props_ty>>::render(*boxed, children)
            }

            #[doc(hidden)]
            unsafe fn __rsx_vtable_drop_props_shim(props: ::core::ptr::NonNull<()>) {
                drop(unsafe {
                    ::std::boxed::Box::from_raw(props.as_ptr().cast::<#strict_props_ty>())
                });
            }

            #[doc(hidden)]
            unsafe fn __rsx_vtable_clone_props_shim(
                props: ::core::ptr::NonNull<()>,
            ) -> ::core::ptr::NonNull<()> {
                let source: &#strict_props_ty = unsafe {
                    &*props.as_ptr().cast::<#strict_props_ty>()
                };
                let cloned = <#strict_props_ty as ::core::clone::Clone>::clone(source);
                let boxed = ::std::boxed::Box::new(cloned);
                let raw = ::std::boxed::Box::into_raw(boxed);
                ::core::ptr::NonNull::new(raw.cast())
                    .expect("Box::into_raw returns non-null")
            }
        }

        impl #impl_generics ::rfgui::ui::ComponentTag for #self_ty #where_clause {
            const VTABLE: &'static ::rfgui::ui::ComponentVTable =
                &::rfgui::ui::ComponentVTable {
                    render: <#self_ty>::__rsx_vtable_render_shim,
                    drop_props: <#self_ty>::__rsx_vtable_drop_props_shim,
                    clone_props: <#self_ty>::__rsx_vtable_clone_props_shim,
                    props_eq: ::core::option::Option::None,
                    type_name: #type_name_str,
                };
        }
    }
}

fn expand_component(input_fn: ItemFn) -> proc_macro2::TokenStream {
    let vis = &input_fn.vis;
    let comp_name = &input_fn.sig.ident;
    let helper_name = format_ident!("__rsx_component_impl_{}", comp_name);
    let props_name = format_ident!("{}Props", comp_name);
    let init_name = format_ident!("__{}Init", props_name);
    let fn_generics = &input_fn.sig.generics;
    let (impl_generics, ty_generics, where_clause) = fn_generics.split_for_impl();
    let has_generics = !input_fn.sig.generics.params.is_empty();

    let output_ty = match &input_fn.sig.output {
        ReturnType::Default => quote!(::rfgui::ui::RsxNode),
        ReturnType::Type(_, ty) => quote!(#ty),
    };

    let mut prop_fields = Vec::new();
    let mut helper_args = Punctuated::<FnArg, Token![,]>::new();
    let mut helper_call_args = Vec::new();
    let mut accepts_children = false;
    let mut init_fields = Vec::new();
    let mut init_default_fields = Vec::new();
    let mut from_init_fields = Vec::new();

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
            // `children` is the only rsx-semantic reserved param: it maps to
            // the second positional arg of `RsxComponent::render`. The macro
            // does not constrain its declared type — the generated
            // `render(props, children)` call fails at the `RsxComponent` trait
            // bound if the user's type is incompatible, which is the right
            // place for that error.
            accepts_children = true;
            helper_args.push(parse_quote!(#field_ident: #ty));
            helper_call_args.push(quote!(children));
            continue;
        } else {
            ty.clone()
        };
        prop_fields.push(quote!(pub #field_ident: #props_field_ty));
        let (init_inner, is_already_option) = match option_inner_type(&props_field_ty) {
            Some(inner) => (inner.clone(), true),
            None => (props_field_ty.clone(), false),
        };
        init_fields.push(quote! {
            pub #field_ident: ::core::option::Option<#init_inner>,
        });
        init_default_fields.push(quote! {
            #field_ident: ::core::option::Option::None,
        });
        if is_already_option {
            from_init_fields.push(quote! {
                #field_ident: __init.#field_ident,
            });
        } else {
            let field_name = field_ident.to_string();
            let comp_name_str = comp_name.to_string();
            from_init_fields.push(quote! {
                #field_ident: __init.#field_ident.expect(concat!(
                    "missing required prop `",
                    #field_name,
                    "` on <",
                    #comp_name_str,
                    ">"
                )),
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

        // React parity P2: Props must be `Clone` so the `unwrap_components`
        // walker can handle shared `Rc<ComponentNodeInner>` (e.g. when the
        // caller extracts a subtree into a variable and embeds it in two
        // places, or when the memo cache replays a prior render).
        #[derive(::core::clone::Clone)]
        #vis struct #props_name #fn_generics {
            #(#prop_fields,)*
        }

        #[doc(hidden)]
        #vis struct #init_name #fn_generics {
            #(#init_fields)*
        }

        impl #impl_generics ::core::default::Default for #init_name #ty_generics #where_clause {
            fn default() -> Self {
                Self {
                    #(#init_default_fields)*
                }
            }
        }

        impl #impl_generics ::core::convert::From<#init_name #ty_generics> for #props_name #ty_generics #where_clause {
            fn from(__init: #init_name #ty_generics) -> Self {
                Self {
                    #(#from_init_fields)*
                }
            }
        }

        impl #impl_generics ::rfgui::ui::RsxComponent<#props_name #ty_generics> for #comp_name #ty_generics #where_clause {
            fn render(props: #props_name #ty_generics, children: ::std::vec::Vec<::rfgui::ui::RsxNode>) -> ::rfgui::ui::RsxNode {
                let _ = &children;
                #helper_name(#(#helper_call_args),*)
            }
        }

        impl #impl_generics ::rfgui::ui::RsxTag for #comp_name #ty_generics #where_clause {
            type Props = #init_name #ty_generics;
            type StrictProps = #props_name #ty_generics;
            const ACCEPTS_CHILDREN: bool = #accepts_children;

            fn into_strict(props: Self::Props) -> Self::StrictProps {
                ::core::convert::From::from(props)
            }

            fn create_node(
                props: Self::StrictProps,
                children: ::std::vec::Vec<::rfgui::ui::RsxNode>,
                _key: ::core::option::Option<::rfgui::ui::RsxKey>,
            ) -> ::rfgui::ui::RsxNode {
                <#comp_name #ty_generics as ::rfgui::ui::RsxComponent<#props_name #ty_generics>>::render(props, children)
            }

            fn component_vtable() -> ::core::option::Option<&'static ::rfgui::ui::ComponentVTable> {
                ::core::option::Option::Some(
                    <Self as ::rfgui::ui::ComponentTag>::VTABLE,
                )
            }
        }

        // React parity P0: compile-time type-erased dispatch shims.
        // Dead code until P2 wires the `RsxNode::Component` deferred path.
        // Each shim is monomorphized per concrete component type, so the
        // unsafe cast back to `#props_name` is always well-typed.
        #[allow(non_snake_case, dead_code)]
        impl #impl_generics #comp_name #ty_generics #where_clause {
            #[doc(hidden)]
            unsafe fn __rsx_vtable_render_shim(
                props: ::core::ptr::NonNull<()>,
                children: ::std::vec::Vec<::rfgui::ui::RsxNode>,
            ) -> ::rfgui::ui::RsxNode {
                let boxed: ::std::boxed::Box<#props_name #ty_generics> =
                    unsafe { ::std::boxed::Box::from_raw(props.as_ptr().cast()) };
                <#comp_name #ty_generics as ::rfgui::ui::RsxComponent<#props_name #ty_generics>>::render(*boxed, children)
            }

            #[doc(hidden)]
            unsafe fn __rsx_vtable_drop_props_shim(props: ::core::ptr::NonNull<()>) {
                drop(unsafe {
                    ::std::boxed::Box::from_raw(props.as_ptr().cast::<#props_name #ty_generics>())
                });
            }

            #[doc(hidden)]
            unsafe fn __rsx_vtable_clone_props_shim(
                props: ::core::ptr::NonNull<()>,
            ) -> ::core::ptr::NonNull<()> {
                let source: &#props_name #ty_generics = unsafe {
                    &*props.as_ptr().cast::<#props_name #ty_generics>()
                };
                let cloned = <#props_name #ty_generics as ::core::clone::Clone>::clone(source);
                let boxed = ::std::boxed::Box::new(cloned);
                let raw = ::std::boxed::Box::into_raw(boxed);
                ::core::ptr::NonNull::new(raw.cast())
                    .expect("Box::into_raw returns non-null")
            }
        }

        impl #impl_generics ::rfgui::ui::ComponentTag for #comp_name #ty_generics #where_clause {
            const VTABLE: &'static ::rfgui::ui::ComponentVTable =
                &::rfgui::ui::ComponentVTable {
                    render: <Self>::__rsx_vtable_render_shim,
                    drop_props: <Self>::__rsx_vtable_drop_props_shim,
                    clone_props: <Self>::__rsx_vtable_clone_props_shim,
                    props_eq: ::core::option::Option::None,
                    type_name: ::core::stringify!(#comp_name),
                };
        }

        #[allow(non_snake_case)]
        fn #helper_name #helper_generics (#helper_args) -> #output_ty #body
    }
}


// ============================================================================
// v2 expansion: React-style shared createElement path
// ============================================================================

fn expand_node(child: &Child) -> proc_macro2::TokenStream {
    match child {
        Child::Element(element) => expand_element(element),
        Child::TextLiteral(text) => quote! { ::rfgui::ui::RsxNode::text(#text) },
        Child::TextRaw(text) => quote! { ::rfgui::ui::RsxNode::text(#text) },
        Child::Expr(expr) => quote! { ::rfgui::ui::IntoRsxNode::into_rsx_node(#expr) },
    }
}

fn expand_child_append(child: &Child) -> proc_macro2::TokenStream {
    match child {
        Child::Expr(expr) => quote! {
            ::rfgui::ui::append_rsx_child_node(&mut __rsx_children, #expr);
        },
        _ => {
            let node = expand_node(child);
            quote! { __rsx_children.push(#node); }
        }
    }
}

fn expand_element(element: &ElementNode) -> proc_macro2::TokenStream {
    let tag = &element.tag;
    let has_children = !element.children.is_empty();
    let child_appends = element.children.iter().map(expand_child_append);

    let parent_path = quote!(__init);
    let prop_assignments = element
        .props
        .iter()
        .filter(|p| p.key != "key")
        .map(|prop| expand_prop_assignment(prop, &parent_path));

    // Hoist `Missing`-style / incomplete entry diagnostics to the top of the
    // element block. Emitting `compile_error!` from deep inside the init
    // closure works for rustc but rust-analyzer routinely loses the span
    // mapping and plants the squiggle on the whole `rsx!` invocation. At
    // block-top level r-a tracks the span far more reliably.
    let mut diagnostics: Vec<proc_macro2::TokenStream> = element.diagnostics.clone();
    for prop in &element.props {
        collect_prop_missing_errors(prop, &mut diagnostics);
    }
    let component_key = component_key_tokens(element);
    let children_schema_check = if has_children {
        quote! {
            let _: [(); 1] = [(); <#tag as ::rfgui::ui::RsxTag>::ACCEPTS_CHILDREN as usize];
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

    // `PhantomData::<#close_tag>` nudges rustc / rust-analyzer to resolve
    // the closing-tag name (enables hover, goto-def, and unused-import
    // warnings on the close ident). When the user drops generics on close
    // (`<Provider::<Ctx>>...</Provider>`), the bare `Provider` path no
    // longer type-checks; fall back to the open tag in that case. Close
    // tag ident still carries its own span for mismatch diagnostics
    // emitted during parse.
    let close_phantom_tag = if close_tag_has_args(&element.close_tag) {
        element.close_tag.clone()
    } else {
        element.tag.clone()
    };

    quote! {
        {
            #(#diagnostics)*
            let _ = ::core::marker::PhantomData::<#close_phantom_tag>;
            #children_schema_check
            ::rfgui::ui::__rsx_create_element::<#tag, _>(
                |__init: &mut <#tag as ::rfgui::ui::RsxTag>::Props| {
                    #(#prop_assignments)*
                },
                #children_value,
                #component_key,
            )
        }
    }
}

/// Walks a prop value and pushes a `compile_error!` for every `Missing`
/// style/object entry onto `out`. Emitted at element-block top level so
/// rust-analyzer's span mapping survives (see `expand_element`).
fn collect_prop_missing_errors(prop: &Prop, out: &mut Vec<proc_macro2::TokenStream>) {
    if let PropValueExpr::Object(entries) = &prop.value {
        for e in entries {
            collect_object_entry_missing(e, out);
        }
    }
}

fn collect_object_entry_missing(entry: &ObjectEntry, out: &mut Vec<proc_macro2::TokenStream>) {
    match &entry.value {
        ObjectValueExpr::Missing => {
            out.push(
                syn::Error::new(
                    entry.key.span(),
                    "value is incomplete; expected `:` and a value",
                )
                .to_compile_error(),
            );
        }
        ObjectValueExpr::Object(inner) => {
            for e in inner {
                collect_object_entry_missing(e, out);
            }
        }
        ObjectValueExpr::Expr(_) => {}
    }
}

fn expand_prop_assignment(
    prop: &Prop,
    parent_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let key = &prop.key;
    let key_span = key.span();
    match &prop.value {
        PropValueExpr::Missing => {
            // `<Element disabled />` shorthand — set to `Some(true)`.
            quote_spanned! {key_span=>
                #parent_path.#key = ::core::option::Option::Some(true);
            }
        }
        PropValueExpr::Expr(expr) => {
            if let Some(tokens) = expand_event_closure_assignment(key, expr, parent_path) {
                return tokens;
            }
            quote_spanned! {key_span=>
                #parent_path.#key = ::rfgui::ui::IntoOptionalProp::into_optional_prop(#expr);
            }
        }
        PropValueExpr::Macro(tokens) => quote_spanned! {key_span=>
            #parent_path.#key = ::rfgui::ui::IntoOptionalProp::into_optional_prop(#tokens);
        },
        PropValueExpr::Object(entries) => expand_object_literal(key, parent_path, entries),
        PropValueExpr::Invalid(error_tokens) => quote! {
            #error_tokens
        },
    }
}

fn expand_object_literal(
    key: &Ident,
    parent_path: &proc_macro2::TokenStream,
    entries: &[ObjectEntry],
) -> proc_macro2::TokenStream {
    let inner_parent = quote!(__obj);
    let inner_assignments: Vec<proc_macro2::TokenStream> = entries
        .iter()
        .map(|e| expand_object_entry_assignment(e, &inner_parent))
        .collect();

    quote_spanned! {key.span()=>
        #parent_path.#key = ::core::option::Option::Some({
            let mut __obj = ::rfgui::ui::__rsx_default_inner_option(&#parent_path.#key);
            #(#inner_assignments)*
            __obj
        });
    }
}

// Expand a value expression into an assignment to `parent.key`, with special
// rewrite for `if`/`match`/`block` where a branch is `None` — those branches
// become a no-op instead of evaluating to `None`, so the user can write
// `color: if cond { Color::X } else { None }` where the else arm is literal None.
fn expand_assignment_with_none_rewrite(
    parent_path: &proc_macro2::TokenStream,
    key: &Ident,
    expr: &Expr,
) -> proc_macro2::TokenStream {
    if is_none_expr(expr) {
        return quote! {};
    }
    if let Some(tokens) = rewrite_conditional_assignment(parent_path, key, expr) {
        return tokens;
    }
    quote! {
        #parent_path.#key =
            ::rfgui::ui::IntoOptionalProp::into_optional_prop(#expr);
    }
}

fn rewrite_conditional_assignment(
    parent_path: &proc_macro2::TokenStream,
    key: &Ident,
    expr: &Expr,
) -> Option<proc_macro2::TokenStream> {
    if is_none_expr(expr) {
        return Some(quote! {});
    }
    if let Expr::If(expr_if) = expr {
        let then_expr = block_single_expr(&expr_if.then_branch)?;
        let cond = &expr_if.cond;
        let then_tokens = rewrite_conditional_assignment(parent_path, key, then_expr)?;
        if let Some((_, else_expr)) = &expr_if.else_branch {
            let else_tokens = rewrite_conditional_assignment(parent_path, key, else_expr)?;
            return Some(quote! {
                if #cond {
                    #then_tokens
                } else {
                    #else_tokens
                }
            });
        }
        return Some(quote! {
            if #cond { #then_tokens }
        });
    }
    if let Expr::Block(expr_block) = expr
        && let Some(inner) = block_single_expr(&expr_block.block)
    {
        return rewrite_conditional_assignment(parent_path, key, inner);
    }
    if let Expr::Match(expr_match) = expr {
        let match_expr = &expr_match.expr;
        let arms: Option<Vec<_>> = expr_match
            .arms
            .iter()
            .map(|arm| {
                let pat = &arm.pat;
                let guard = arm
                    .guard
                    .as_ref()
                    .map(|(_, guard_expr)| quote! { if #guard_expr });
                let body = rewrite_conditional_assignment(parent_path, key, &arm.body)?;
                Some(quote! {
                    #pat #guard => { #body }
                })
            })
            .collect();
        let arms = arms?;
        return Some(quote! {
            match #match_expr {
                #(#arms),*
            }
        });
    }
    Some(quote! {
        #parent_path.#key =
            ::rfgui::ui::IntoOptionalProp::into_optional_prop(#expr);
    })
}

fn expand_object_entry_assignment(
    entry: &ObjectEntry,
    parent_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let key = &entry.key;
    match &entry.value {
        // Error hoisted to element-block top by `collect_prop_missing_errors`.
        // Emit a field access at the key span so rust-analyzer resolves the
        // partial ident as a field of the enclosing struct (completions:
        // `w` -> `width`).
        ObjectValueExpr::Missing => quote_spanned! {key.span()=>
            let _ = &#parent_path.#key;
        },
        ObjectValueExpr::Expr(expr) => expand_assignment_with_none_rewrite(parent_path, key, expr),
        ObjectValueExpr::Object(entries) => expand_object_literal(key, parent_path, entries),
    }
}

#[cfg(test)]
mod tests {
    use super::{MultipleNodes, ObjectValueExpr, PropValueExpr, expand_node};
    use quote::ToTokens;

    #[test]
    fn close_tag_may_omit_generics_on_open() {
        // React-parity: `<Provider::<T>>…</Provider>` must parse. Close tag
        // path key compares idents only; PhantomData fallback uses open
        // tag when close has no args.
        syn::parse_str::<MultipleNodes>(
            r#"<Provider::<Ctx> value={v}><Child/></Provider>"#,
        )
        .expect("bare close tag should match generic open tag");
    }

    #[test]
    fn close_tag_ident_mismatch_still_rejected() {
        // Stripping generics must not loosen ident comparison.
        let result = syn::parse_str::<MultipleNodes>(
            r#"<Provider::<Ctx> value={v}><Child/></Consumer>"#,
        );
        assert!(result.is_err(), "close ident mismatch should still fail");
    }

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
        let PropValueExpr::Object(entries) = &style_prop.value else {
            panic!("expected object");
        };
        let entry = entries.last().expect("missing recovered style entry");
        assert_eq!(entry.key.to_string(), "backg");
        assert!(matches!(entry.value, ObjectValueExpr::Missing));
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
        let PropValueExpr::Object(entries) = &style_prop.value else {
            panic!("expected object");
        };
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key.to_string(), "backg");
        assert!(matches!(entries[0].value, ObjectValueExpr::Missing));
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
            r#"<Element on_pointer_down={resize_bottom_down />}</Element><Label />"#,
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
            expanded.contains("invalid Rust expression for prop `on_pointer_down` inside `{...}`")
        );

        match &parsed.nodes[1] {
            super::Child::Element(node) => {
                assert_eq!(node.tag.to_token_stream().to_string(), "Label");
            }
            _ => panic!("expected second node to be element"),
        }
    }

    #[test]
    fn style_object_prop_expands_via_default_inner_option() {
        let parsed = syn::parse_str::<MultipleNodes>(
            r##"<Text style={{ color: Color::hex("#fff") }}>{"A"}</Text>"##,
        )
        .expect("rsx should parse text style");

        let expanded = expand_node(&parsed.nodes[0]).to_string();
        assert!(expanded.contains("Text"));
        assert!(expanded.contains("__rsx_default_inner_option"));
    }

    #[test]
    fn nested_object_prop_expands_via_default_inner_option() {
        let parsed = syn::parse_str::<MultipleNodes>(
            r##"<Window window_slots={{ root_style: { background: Color::hex("#fff") } }} />"##,
        )
        .expect("rsx should parse nested object prop");

        let expanded = expand_node(&parsed.nodes[0]).to_string();
        // Both outer `window_slots` and inner `root_style` expand through
        // the shared `__rsx_default_inner_option` helper; the helper is
        // called once per nesting level.
        assert_eq!(expanded.matches("__rsx_default_inner_option").count(), 2);
    }

    #[test]
    fn rsx_expansion_uses_create_element_path() {
        let parsed = syn::parse_str::<MultipleNodes>(r#"<Element />"#).expect("rsx should parse");

        let expanded = expand_node(&parsed.nodes[0]).to_string();
        assert!(expanded.contains("__rsx_create_element"));
    }
}
