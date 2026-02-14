use proc_macro::TokenStream;

mod tool;

/// Mark an `impl` block as a Tool.
///
/// Example:
/// ```ignore
/// #[tool(id="debug", description="Debug utilities")]
/// impl DebugTool {
///     #[tool_fn(name="debug-echo")]
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

#[proc_macro_attribute]
pub fn tool_arg(attr: TokenStream, item: TokenStream) -> TokenStream {
    // This attribute is a marker consumed by #[tool] while expanding the impl.
    // It must *not* remain as an attribute macro on the parameter, otherwise rustc
    // will reject it as "expected non-macro attribute".
    let _ = attr;
    item
}
