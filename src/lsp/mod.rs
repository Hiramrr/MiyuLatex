//! Cliente mínimo de LSP: diagnósticos, ir a la definición y hover.

mod client;
mod hub;
mod position;
mod rpc;
mod servers;

pub use hub::{DocRef, Hub, Outcome, Severity, State};
pub use position::char_col;
#[cfg(test)]
pub use servers::{Launch, Spec};

#[cfg(test)]
pub(crate) mod tests;
