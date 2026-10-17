use heck::ToPascalCase;
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::{
    Error, FnArg, Ident, ItemTrait, Pat, Path, Result as SResult, ReturnType, TraitItem, Type,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
};

struct ActorPaths {
    actor: Path,
    embassy_sync: Path,
}

fn resolve_actor_paths() -> ActorPaths {
    match crate_name("ivy") {
        Ok(FoundCrate::Itself) => ActorPaths {
            actor: parse_quote!(crate::actor),
            embassy_sync: parse_quote!(crate::__private::embassy_sync),
        },

        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name.replace('-', "_"));

            ActorPaths {
                actor: parse_quote!(::#ident::__private::types::actor),
                embassy_sync: parse_quote!(::#ident::__private::embassy_sync),
            }
        }

        Err(_) => ActorPaths {
            actor: parse_quote!(::ivy_types::actor),
            embassy_sync: parse_quote!(::embassy_sync),
        },
    }
}

struct ActorMacroArgs {
    handle_ident: Ident,
}

impl Parse for ActorMacroArgs {
    fn parse(input: ParseStream<'_>) -> SResult<Self> {
        let handle_ident = input.parse()?;

        if !input.is_empty() {
            return Err(input.error("unexpected tokens after handle name"));
        }

        Ok(Self { handle_ident })
    }
}

struct MethodInfo {
    name: Ident,
    variant: Ident,
    args: Vec<(Ident, Type)>,
    ret: Type,
}

pub fn expand_handle(attrs: TokenStream, item: TokenStream) -> TokenStream {
    let trait_item = parse_macro_input!(item as ItemTrait);
    let args = parse_macro_input!(attrs as ActorMacroArgs);

    expand_handle_impl(args, trait_item).unwrap_or_else(Error::into_compile_error).into()
}

fn expand_handle_impl(args: ActorMacroArgs, trait_item: ItemTrait) -> SResult<proc_macro2::TokenStream> {
    let paths = resolve_actor_paths();
    let actor = &paths.actor;
    let embassy_sync = &paths.embassy_sync;

    let trait_name = &trait_item.ident;
    let command_name = format_ident!("{}Command", trait_name);
    let handle_name = args.handle_ident;

    let generics = &trait_item.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let methods: Vec<MethodInfo> = trait_item
        .items
        .iter()
        .filter_map(|item| match item {
            TraitItem::Fn(method) => Some(method),
            _ => None,
        })
        .map(|method| {
            let sig = &method.sig;

            let ret = match &sig.output {
                ReturnType::Default => parse_quote!(()),
                ReturnType::Type(_, ty) => (**ty).clone(),
            };

            let args = sig
                .inputs
                .iter()
                .filter_map(|arg| match arg {
                    FnArg::Typed(pat) => match &*pat.pat {
                        Pat::Ident(id) => Some((id.ident.clone(), (*pat.ty).clone())),
                        _ => None,
                    },
                    FnArg::Receiver(_) => None,
                })
                .collect();

            MethodInfo {
                name: sig.ident.clone(),
                variant: format_ident!("{}", sig.ident.to_string().to_pascal_case()),
                args,
                ret,
            }
        })
        .collect();

    // 1. Generate Command enum.
    let command_enum = {
        let variants = methods.iter().map(|method| {
            let variant = &method.variant;
            let tys = method.args.iter().map(|(_, ty)| ty);
            let ret = &method.ret;

            quote! {
                #variant(
                    #actor::ReplyConsumer<#ret>,
                    #(#tys,)*
                )
            }
        });

        quote! {
            pub enum #command_name #generics #where_clause {
                #(#variants),*
            }
        }
    };

    // 2. Generate handle methods.
    let handle_methods = methods.iter().map(|method| {
        let name = &method.name;
        let variant = &method.variant;
        let ret = &method.ret;

        let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();

        let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

        quote! {
            pub async fn #name(
                &self
                #(, #arg_names: #arg_types)*
            ) -> #ret {
                #actor::rt::request(
                    &self.cmd_tx,
                    |c| #command_name::#variant(
                        c,
                        #(#arg_names,)*
                    )
                ).await
            }
        }
    });

    // 3. Generate handle struct and ActorHandle implementation.
    let handle = quote! {
        #[derive(Clone)]
        pub struct #handle_name #generics #where_clause {
            cmd_tx: #embassy_sync::channel::DynamicSender<
                'static,
                #command_name #ty_generics,
            >,
        }

        impl #impl_generics #handle_name #ty_generics #where_clause {
            #(#handle_methods)*
        }

        impl #impl_generics #actor::ActorHandle
            for #handle_name #ty_generics
            #where_clause
        {
            type Cmd = #command_name #ty_generics;

            fn create(
                cmd_tx: #embassy_sync::channel::DynamicSender<
                    'static,
                    #command_name #ty_generics,
                >,
            ) -> Self {
                Self { cmd_tx }
            }
        }
    };

    Ok(quote! {
        #command_enum
        #handle
    })
}
