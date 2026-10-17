use proc_macro::TokenStream;

mod actor;

/// Macro that is used to generate actor handle based on trait functions, here's a quick description of how it works:
/// ```ignore
/// #[actor_handle(SomeActorHandleName)] // Provide name for generated structure
/// trait SomeActorHandle {
///     async fn make_bar(&self, foo: Foo) -> Bar;
/// }
/// // That expands to:
///
/// // Enum name is always `<TraitName>Commands`
/// enum SomeActorHandleCommands {
///     MakeBar(ReplyConsumer<Bar>, Foo) // ReplyConsumer is just a consumer that will deliver the result
/// }
///
/// struct SomeActorHandleName {
///     cmd_channel: Channel<SomeActorHandleCommands>
/// }
///
/// impl SomeActorHandleName {
///     async fn make_bar(&self, foo: Foo) -> Bar {
///         use ivy_types::actor::rt::{request, create_consumer};
///         request(SomeActorHandleCommands::MakeBar(create_consumer(), foo)) // Simplified, but that's the gist
///     }
/// }
/// ```
///
/// On the actor side, you match on the generated enum in your actor loop
/// and send the result back through the `ReplyConsumer`.
#[proc_macro_attribute]
pub fn actor_handle(attr: TokenStream, item: TokenStream) -> TokenStream {
    actor::expand_handle(attr, item)
}
