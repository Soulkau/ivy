use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Error, Ident, LitStr, Path, Result, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct SubscriptionInput {
    ivy: Path,
    topic: LitStr,
    ty: Type,
    codec: Path,
}

impl Parse for SubscriptionInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        // The facade path is supplied by the macro_rules! wrapper.
        let ivy: Path = input.parse()?;
        input.parse::<Token![,]>()?;

        let topic: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let ty: Type = input.parse()?;

        // Default codec.
        let mut codec: Path = syn::parse_quote!(#ivy::__private::core::codecs::Json);
        let mut codec_seen = false;

        while !input.is_empty() {
            input.parse::<Token![,]>()?;

            // Allow a trailing comma.
            if input.is_empty() {
                break;
            }

            let option: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match option.to_string().as_str() {
                "codec" => {
                    if codec_seen {
                        return Err(Error::new(option.span(), "duplicate `codec` option"));
                    }

                    codec = input.parse()?;
                    codec_seen = true;
                }
                _ => {
                    return Err(Error::new(option.span(), format!("unknown subscription option `{option}`")));
                }
            }
        }

        Ok(Self { ivy, topic, ty, codec })
    }
}

pub fn expand_subscription(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SubscriptionInput);

    let ivy = input.ivy;
    let topic = input.topic;
    let ty = input.ty;
    let codec = input.codec;

    quote! {
        {
            static __SIGNAL: #ivy::__private::embassy_sync::signal::Signal<
                #ivy::__private::embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
                #ty,
            > = #ivy::__private::embassy_sync::signal::Signal::new();

            static __SUB: #ivy::__private::static_cell::StaticCell<
                #ivy::__private::core::mqtt::subscription::Subscription<
                    #ty,
                    #codec,
                >
            > = #ivy::__private::static_cell::StaticCell::new();

            &*__SUB.init_with(|| {
                #ivy::__private::core::mqtt::subscription::Subscription::<
                    #ty,
                    #codec,
                >::new(#topic, &__SIGNAL)
            })
        }
    }
    .into()
}
