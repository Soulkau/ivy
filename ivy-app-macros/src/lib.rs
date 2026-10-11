use proc_macro::TokenStream;

mod mqtt;

#[proc_macro]
pub fn subscribe(input: TokenStream) -> TokenStream {
    mqtt::expand_subscription(input)
}
