use proc_macro::TokenStream;

mod subscriber;

#[proc_macro]
pub fn __subscribe_internal(input: TokenStream) -> TokenStream {
    subscriber::expand_subscription(input)
}
