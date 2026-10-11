use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Error, Expr, Ident, LitStr, Path, Result, Token, Type,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct SubscriptionInput {
    topic: LitStr,
    _comma: Token![,],
    ty: Type,
    codec: Path,
}

impl Parse for SubscriptionInput {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let topic: LitStr = input.parse()?;
        let comma: Token![,] = input.parse()?;
        let ty: Type = input.parse()?;

        // Default codec.
        let mut codec: Path = syn::parse_quote!(Json);
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

        Ok(Self { topic, _comma: comma, ty, codec })
    }
}

pub fn expand_subscription(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SubscriptionInput);

    let topic = input.topic;
    let ty = input.ty;
    let codec = input.codec;

    let expanded = quote! {
        {
            static __SIGNAL: ::embassy_sync::signal::Signal<
                ::embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
                #ty,
            > = ::embassy_sync::signal::Signal::new();

            static __SUB: ::static_cell::StaticCell<ivy_core::mqtt::subscription::Subscription::<#ty, #codec>> = ::static_cell::StaticCell::new();

            SUB.init_with(|| ::ivy_core::mqtt::subscription::Subscription::<#ty, #codec>::new(#topic, &SIGNAL))
        }
    };

    expanded.into()
}
