use proc_macro::TokenStream;

mod tool;

/// Mark an `impl` block as a Tool.
///
/// Example:
/// ```ignore
/// #[tool(id="debug", description="Debug utilities")]
/// impl DebugTool {
///     #[tool_fn(name="debug.echo")]
///     async fn echo(&self, text: String) -> anyhow::Result<String> { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
    tool::tool(attr, item)
}

/// Mark an async method inside a `#[tool] impl` as a tool function.
#[proc_macro_attribute]
pub fn tool_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Marker attribute: we parse it later from the impl block.
    item
}

/// Mark a parameter for a `#[tool_fn]` method.
#[proc_macro_attribute]
pub fn tool_arg(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Marker attribute: we parse it later from the method signature.
    item
}
