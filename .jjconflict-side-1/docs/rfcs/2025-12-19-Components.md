# Smarter Components

Author: Leander Beernaert

Approvers: @et-rust-devs

Jira: JIRA EPIC/TASK TICKET ONCE RFC IS APPROVED

## Objective

> What is a short summary?

Migrate even more of the business logic that is currently replicated between clients and simplify the 
usage of each of the code surface that powers the UI.

## Background & Motivation

> Why do we want to do this? What are we trying to solve? What this is NOT solving?

Over the course of the development of ET mail we took a "pure" SDK oriented approach to how clients should
integrate the rust code. This has led to some things that each client still needs to replicate as well as 
some complexities that could be hidden from them.

For instance, the centerpiece of the mail application is the `Mailbox` component. Currently, the clients need to:

* Create a `Mailbox`.
* Create a `MailScroller` that is either conversation or message based, depending on a property of the mailbox
  and manage callback lifetime of the scroller.
* Create a watcher for unread label count and manage callback lifetime.
* Remember the value of the display filter (Show trashed/spam messages)
* Recreate `Mailbox` on label change and all of the above
* Display different recipients based on the label
* Retry reload if there is no network

The above are all things that can be done by rust and avoid replicating this between all clients.
Other components that would benefit from this approach are:

* Composer
* Sidebar
* Conversation View
* Message View

In this RFC we propose an alternate way to handle all of this in way that simplifies the client integration and 
gives the rust developers more tools to crate feature rich components.

## Benefits

> How will users or devs or our system benefit from those changes?

This will reduce the friction for integrating components on the various clients as well as further reduce
the amount of business logic that needs to be repeated.

## Proposal

> How do we want to solve it?

In essence each of these components will be turned into an actor with an event stream. 

```
                 │           ┌─────────────────┐     
┌──────────┐     │           │                 │     
│          │     │           │                 │     
│  Actor   │◄────┼──────┐    │                 │     
│  Event   │     │      │    │            ┌────┴────┐
│  Stream  │     │      │    │            │         │
│          │     │      │    │            │  Async  │
└──────────┘     │      │    │            │  Task   │
                 │     ┌┴────▼───┐        │         │
┌──────────┐     │     │         │        └─────────┘
│          │     │     │         │                   
│  Actor   ├─────┼────►│  Actor  │                   
│  Handle  │     │     │         │                   
│          │     │     │         │        ┌─────────┐
└──────────┘     │     └▲───▲────┘        │         │
                 │      │   │             │   DB    │
                 │      │   │             │ Watcher │
                 │      │   └─────────────┼         │
                 │      │                 └─────────┘
                 │      │                            
                 │   ┌──┴───────────────────┐        
                 │   │                      │        
                 │   │   Other Sources of   │        
                 │   │   Input/Data         │        
                     │                      │        
                     └──────────────────────┘                                          
```

### Actor

Each component will be launched on a background task (e.g.: `tokio::spawn`) and all communication will happen via 
message passing. 

This setup allows the component to be driven by either input from the clients or react to events produces from
other sources such as background tasks, database watchers, network monitors, timers, etc...

Another advantage of this setup is that it removes the problems we have in uniffi where we need to schedule
things on the tokio runtime for them to work correctly.

### Event Stream

Rather than registering multiple callbacks, on construction the integrator receives a stream which will 
be the sole source of receiving updates.

```rust
let (actor, stream) =  Component::new(...);
```

This event stream should contain all relevant updates related to visual or internal state.

E.g.
```rust
enum MailboxEvent {
    Initializing,
    UnreadCounterUpdate(u64),
    Scroller(ScrollerEvent),
    ViewOptions(...),
    // etc...
}
```

To facilitate integration with existing clients setups, it should be possible to request more than one copy of the stream.

A good implementation candidate for this would be tokio's broadcast channels. However, rather than ignoring 
the `Err(RecvLagged)` error, the stream will be terminated and the integrator should reload all state and acquire
a new stream to maintain consistency.

## Examples

> What will change? How does it impact UI/UX?

### Drafts/Composer

The current draft type internally already behaves this way, we just need to replace all the existing callbacks 
which are maintained via a compatability layer with direct access to the underlying event stream.

### Message Viewer

The message viewer component would encompass:

* Fetching and retrieval of the message data
* auto retry when network is restored if offline
* action bar updates
* message updates

### Conversation Viewer

The conversation viewer component would encompass:

* Management of conversation messages
* action bar updates
* conversation and message updates
* Auto retry when network is offline

### Mailbox

This component will see the most changes and will move a lot of scattered logic into it. 

The mailbox will be updated to handle the following:

* Subscribe to unread counter updates
* Handle conversation vs messages view mode
  * This would be returned as an enum of either conversations or messages (see below)
* Offline auto retry
* Label changes: Rather than creating a new mailbox, the internal state would reset and 
  appropriate events would be issued to reflect that.
* Handling of recipient display types (Sender or Recipients)
* Conversation filtering option
* Api to load conversations or messages which keep in mind the current state and filtering options.

#### Conversations and Messages

This would be represented via custom display types rather than full data types that we display today in
order to achieve all the UI goals.

E.g.:
```rust
enum ConversationsOrMessages {
    Conversations(Vec<UIConversation>),
    Messages(Vec<UIConversation>),
}

struct UIMessage {
    recipients: Vec<UIRecipient>,// sender or recipients, depending on label and settings
    attachments: Vec<UIAttachments>,
    is_draft:bool,
    location: ...
    etc...
}
```

These would then be returned via scroller events:

```rust
enum MailboxScrollerEvent {
    Clear,
    Append(ConversationOrMessages),
   ...
}

enum MailboxEvent {
  Scroller(MailboxScrollerEvent),
  ...
}
```

#### Better APIs

Rather than having all the loosely coupled functions floating around, the mailbox type will be "enriched" by
method that internally handle all this parameter management.

```rust
impl Mailbox {
    async fn open_conversation(&self, id:LocalConversationId) -> Result<(ConversationViewComponent,ConversationViewEventStream>), ...>;
  
    async fn change_label(&self, id:LocalLabelId) -> Result<(), ...>;
  
    async fn set_conversation_view_option(&self, ...) -> ...;
  
    async fn mark_messages_read(&self, ids:Vec<LocalMessageIds>) -> ...;
    
    // etc...
}
```

There is also an opportunity to have an item agnostic api, which can work for either conversation
or messages when performing multi select actions.

```rust
impl Mailbox {
   async fn mark_read(&self, ids:Vec<MailboxItemId>) ...
}
```

Internally the rust code will decompose to respective types.

### MailContext/Context/MailSession

If necessary this type can also be update to return a global event stream for the application wide events, such as 
network status updates, session creation/destruction, etc...

### MailUserContext/UserContext/MailUserSession

This type would also benefit from having an event stream to report user based events such as event loop errors
and draft send failures. However, since the clients are currently creating these in a myriad of locations, some 
care would need to be taken to ensure certain event are not dropped/ignored.

Some more investigation is required here, it's possible this conversion can't be performed reliably and the 
current set of watchers will have to remain in place.


## Abandoned Ideas

> What else did we consider?

N/A

## Things to consider

### Abuse

> Anything to discuss with the abuse team / how to prevent abuse?

N/A

### API

> What changes to API do we need to make, and how does it affect clients (who do not implement this change)?

N/A

### Bindings

> How does this affect bindings into other languages?

Other than API changes, nothing else will change.

### Compatibility

> Is it forward and backward compatible? If not, how we can roll out the change, or how we can roll it back?

These components should be developed side by side with the old setup since they should just be repackaging existing 
code into a new location. Once the migration to these new components succeeds, the old methods will be erased.

### Crypto

> Any changes to encryption that need validation from the crypto team? How do we validate encrypted data?

N/A

### Dependencies

> What new dependencies we are introducing to the system?

N/A 

### Monitoring

> How we can monitor the new feature functions properly? Should we set up some Sentry, telemetry, Prometheus, etc.?

N/A

### Security/audit

> Any new library or feature used in a new way that needs to be checked by our security team, or does it require to schedule a full audit?

N/A

### Tests

> What tests (unit, integration, e2e, stress, chaos) we need to implement on every client to validate the RFC is implemented correctly? What new seeding (populate) options we need for those tests?

This new component setup would enable use to write better tests for various features.

### Threats

> What other risks this change brings?

N/A

## Questions

> Once feedback is collected, document what questions were raised during discussion of this proposal?

## Implementation timeline

> Once approved, document what is planned to deploy and when, or when it was deployed.
