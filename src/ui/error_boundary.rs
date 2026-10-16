//! Error boundaries: containing a render that panics.
//!
//! [`catch_render_node`] is the primitive. The walker renders its child, and
//! when a render under it panics, discards that attempt and renders what
//! `on_error` returns in its place. The components that failed unmount, so
//! their state retires; the rest of the tree is unaffected. A mount under it
//! that panics, which runs after the walk, also unmounts the child and calls
//! `on_error`, and the boundary shows what its state says from the next
//! frame. Any component can return such a node to act as a boundary,
//! keeping what it shows and when it tries again in its own state, as a
//! React class component does with `getDerivedStateFromError`.
//!
//! [`ErrorBoundary`] is one such component: it keeps showing its fallback
//! until reset.
//!
//! ```ignore
//! rsx! {
//!     <ErrorBoundary
//!         fallback={rsx! { <Text>"Chart unavailable"</Text> }}
//!         reset_key={ResetKey::new(&points_version)}
//!     >
//!         <Chart points={points} />
//!     </ErrorBoundary>
//! }
//! ```
//!
//! Unlike React, the state an `on_error` sets shows from the next frame;
//! what it returns shows in this one. It also takes the place of
//! `componentDidCatch`, so it is the place to log.
//!
//! Outside any boundary, a render or mount panic unmounts the whole tree
//! (see [`render_root`](crate::ui::render_root)). Panics are caught by
//! unwinding, so a build with `panic = "abort"`, the default on wasm, still
//! aborts.

use crate::ui::{
    RsxComponent, RsxErrorBoundaryNode, RsxNode, RsxNodeIdentity, State, component_key_token,
    provide_context_node, use_state,
};
use ::rfgui_rsx::props;
use std::any::Any;
use std::fmt;
use std::hash::Hash;
use std::rc::Rc;

/// A render or mount that panicked under an error boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError {
    message: String,
}

impl RenderError {
    pub(crate) fn from_panic(payload: Box<dyn Any + Send>) -> Self {
        let message = match payload.downcast::<String>() {
            Ok(message) => *message,
            Err(payload) => payload
                .downcast_ref::<&str>()
                .map_or("a component panicked", |message| message)
                .to_string(),
        };
        Self { message }
    }

    /// The panic message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RenderError {}

/// Builds an error boundary node around `child`. When a render under it
/// panics, `on_error` gets the error and returns what renders instead. When
/// a mount under it panics, after the walk, the child unmounts and
/// `on_error` gets the error, but what it returns is discarded: record the
/// error in state to show a fallback from the next frame.
///
/// `on_error` runs after the owning component's body, so it must not call
/// hooks. It may set state, which renders the owner again on the next
/// frame.
pub fn catch_render_node(
    child: RsxNode,
    on_error: impl Fn(&RenderError) -> RsxNode + 'static,
) -> RsxNode {
    RsxNode::ErrorBoundary(Rc::new(RsxErrorBoundaryNode {
        identity: RsxNodeIdentity::new("ErrorBoundary", None),
        child,
        on_error: Rc::new(on_error),
    }))
}

/// Renders its children, and after a render or mount under them panics,
/// its fallback instead, until reset: by a new `reset_key`, or through the
/// [`ErrorBoundaryHandle`] its fallback can read as context.
pub struct ErrorBoundary;

#[derive(Clone)]
#[props]
pub struct ErrorBoundaryProps {
    /// Renders in place of the children while the boundary holds an error.
    /// Without it, nothing does.
    pub fallback: Option<RsxNode>,
    /// Called with each error the boundary catches.
    pub on_error: Option<Rc<dyn Fn(&RenderError)>>,
    /// A change of this value resets the boundary, so it renders its
    /// children again.
    pub reset_key: Option<ResetKey>,
}

impl PartialEq for ErrorBoundaryProps {
    fn eq(&self, other: &Self) -> bool {
        self.fallback == other.fallback
            && self.reset_key == other.reset_key
            && match (&self.on_error, &other.on_error) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            }
    }
}

/// The value an [`ErrorBoundary`] compares to decide whether to reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetKey(u64);

impl ResetKey {
    pub fn new<T: Hash + ?Sized>(value: &T) -> Self {
        Self(component_key_token(value))
    }
}

/// The error an [`ErrorBoundary`] holds, provided as context to its
/// fallback: `use_context::<ErrorBoundaryHandle>()`.
#[derive(Clone)]
pub struct ErrorBoundaryHandle {
    error: RenderError,
    caught: State<Option<Caught>>,
}

impl ErrorBoundaryHandle {
    pub fn error(&self) -> &RenderError {
        &self.error
    }

    /// Clears the error, so the boundary renders its children again on the
    /// next frame.
    pub fn reset(&self) {
        self.caught.set(None);
    }
}

#[derive(Clone, PartialEq)]
struct Caught {
    error: RenderError,
    reset_key: Option<ResetKey>,
}

impl RsxComponent<ErrorBoundaryProps> for ErrorBoundary {
    fn render(props: ErrorBoundaryProps, children: Vec<RsxNode>) -> RsxNode {
        let caught: State<Option<Caught>> = use_state(|| None);
        let reset_key = props.reset_key;
        let fallback = props
            .fallback
            .unwrap_or_else(|| RsxNode::fragment(Vec::new()));
        let show_fallback = {
            let caught = caught.clone();
            move |error: &RenderError| {
                let handle = ErrorBoundaryHandle {
                    error: error.clone(),
                    caught: caught.clone(),
                };
                provide_context_node(handle, fallback.clone())
            }
        };
        match caught.get() {
            Some(held) if held.reset_key == reset_key => return show_fallback(&held.error),
            // A new reset key: try the children again.
            Some(_) => caught.set(None),
            None => {}
        }
        let on_error = props.on_error;
        catch_render_node(RsxNode::fragment(children), move |error| {
            caught.set(Some(Caught {
                error: error.clone(),
                reset_key,
            }));
            if let Some(on_error) = &on_error {
                on_error(error);
            }
            show_fallback(error)
        })
    }
}

#[::rfgui_rsx::component]
impl crate::ui::RsxTag for ErrorBoundary {
    type Props = __ErrorBoundaryPropsInit;
    type StrictProps = ErrorBoundaryProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(p: Self::Props) -> Self::StrictProps {
        p.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        _key: Option<crate::ui::RsxKey>,
    ) -> RsxNode {
        <Self as RsxComponent<ErrorBoundaryProps>>::render(props, children)
    }
}

#[cfg(test)]
mod tests;
