use proc_macro::TokenStream;

mod subscription;

#[proc_macro]
pub fn __subscribe_internal(input: TokenStream) -> TokenStream {
    subscription::expand_subscription(input)
}
