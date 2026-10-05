use proc_macro::TokenStream;

mod actor;

/// Macro that is used to generate actor handle based on trait functions, here is quick description of how it works:
/// ```
/// #[actor_handle(SomeActorHandleName)] //Takes some name that will be used for generated handle.
/// trait SomeActorHandle {
///     async fn make_bar(&self, foo: Foo) -> Bar;
/// }
/// That expands to:
///
/// enum SomeActorCommands {
/// MakeBar(ReplyConsumer<Bar>, Foo) // ReplyConsumer is just consumer, that will deliver result.
/// }
///
/// struct SomeActorHandleName {
///     cmd_channel: Channel<Some>
/// }
///
/// impl SomeActorHandleName {
///     async fn make_bar(&self, foo: Foo) -> Bar {
///         request(SomeActorCommands::MakeBar(create_consumer(), foo)) //This is really simplified, but that is basically it.
///     }
/// }
///
///
///
/// ```
#[proc_macro_attribute]
pub fn actor_handle(attr: TokenStream, item: TokenStream) -> TokenStream {
    actor::expand_handle(attr, item)
}
