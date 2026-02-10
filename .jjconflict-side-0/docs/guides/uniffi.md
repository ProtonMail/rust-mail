# Uniffi Guide

This guide aims to provide some guidance on how to leverage
[uniffi-rs](https://github.com/mozilla/uniffi-rs) to bind Rust into languages supported by that
library.

Uniffi authors provide first class bindings to the following languages:

* Swift
* Kotlin
* Python

These will always be working and up to date. There also other languages listed in their repo, but
those are maintained by a 3rdparty and may not always be up to date or support all the latest
features.

It is highly recommend to read their [user guide](https://mozilla.github.io/uniffi-rs/latest/) first
before reading this document.

## UDL or Proc Macros

One of the first decisions one is faced with is whether to use UDL files or Rust proc macros to
generate the bindings. You can not mix both of these approaches in the same crate.

The [UDL files](https://mozilla.github.io/uniffi-rs/latest/udl_file_spec.html) are the original
method to perform binding generation and the
[proc macros](https://mozilla.github.io/uniffi-rs/latest/proc_macro/index.html) are more idiomatic
Rust approach to achieve the same goal. There are however, some things too keep in mind.

### Proc Macros

The proc macro approach is newer and significantly reduces the amount of boiler plate required
compared to the UDL approach. Additionally, proc macros also support more features that are not
available in the UDL files such as tuple enums and tuple error types.

```rust
#[derive(uniffi::Enum)]
pub enum MyEnum {
    None,
    Str(String), // < -- this variant is not supported in UDL files
    All { s: String, i: i64 }
}

#[derive(uniff::Error)]
pub enum Error {
  Foo,
  Bar(2) // <-- this variant is not supported by UDL files
  Struct {a:i32}
}
```

Using proc macros also requires one to generate the bindings from a compiled binary using [library
mode](https://mozilla.github.io/uniffi-rs/latest/tutorial/foreign_language_bindings.html#running-uniffi-bindgen-using-a-library-file).

We have chosen this approach for ET.

### UDL Files

The only reason to keep using UDL files at this point would be existing code which already uses UDL
files or compatibility with third party languages.

For instance, the [C# language bindigns](https://github.com/NordSecurity/uniffi-bindgen-cs) do not
support the new proc macro approach as of today (2024/06/16).

## How to structure the Project

There are 2 approaches on can take to setup the project. The traditional one is where all the uniffi
exports are contained in one single crate. The other where the uniffi exports are split over
various crates with feature guards.

Regardless of the approach it is recommended that you always crate 2 separate crates, one for all
the Rust code, with uniffi feature guarded exportds, and other only for uniffi exports:

* crate-common
* crate-uniffi

### Single Crate

This approach forces all uniffi exports to be confined into a single crate, regardless of how many
dependencies the project has. Note that you can still pull in other `*-uniffi` crates as required.

If you want to export a type, you need to mirror the type in this crate and provide `From`
implementation that converts between the `common` and `uniffi` crates.

```rust
// Crate foo-common
pub struct Foo {...};

// Crate foo-uniffi

#[derive(uniffi::Record)]
pub struct Foo{..}

impl From<foo_common::Foo> for Foo { ... }


#[uniffi::export]
fn create_foo()-> Foo {
    foo_common::Foo::new().into()
}
```

This is the approach that works best (less issues with custom types) for uniffi and also allows one
to tailor the exported types as needed. **However**, this approach can lead to **too many layers of
conversions**.

Take for instance the following snippet that extends the previous example:

```rust
#[uniffi::export]
fn create_foo_list()-> Vec<Foo> {
    let list: Vec<foo_common::Foo> = ...;
    list.into_iter().map(Into::into()).collect::<Vec<_>>()
}
```

This code requires us to convert `Vec<foo_common::Foo>` into a `Vec<Foo>` which is then finally
converted into an equivalent type in the binding language. Depending on the size of the list and the
complexity of the conversion, this may or may not be an acceptable performance trade off.

Finally, this approach can boilerplate heavy depending on the number of types you need to export.

### Split Crates

In this approach we try to inject the uniffi exports in the same crate that the original type is
defined. Usually you want to feature guard this code so it is not compiled by default and only when
needed.

```rust
// Crate foo-common
#[cfg_attr(feature="unffi",derive(uniffi::Record))]
pub struct Foo {...};

// Crate foo-uniffi

#[uniffi::export]
fn create_foo()-> Foo {
    foo_common::Foo::new()
}
```

This approach requires only **one layer of conversions** from the Rust type to the binding
language, which should be more performant than the 2 step conversion from the single crate approach.

However, this approach does not give you the full flexibility of tailoring the exported types
since both the Rust and exported type have to be identical.

Finally, it is also susceptible some problems with custom types which are discussed later in this
document.

## Records Vs Objects

How to expose your type to Rust depends greatly on the use cases that type needs to fulfill. We will
explore some pros and cons regarding both approaches and what to keep in mind.

### Records

[Records](https://mozilla.github.io/uniffi-rs/latest/udl/structs.html), also known as Dictionaries
in UDL terms, are structs that are exposed without any methods in the binding language.

Even though the binding language potentially has support to add methods to these types, when they
are lifted to their target language all the data is transferred over and no Rust calls can be made
in the future.

This is akin to the following in C:

```c
struct Foo {
    int bar;
};

struct Foo do_foo();
```

The best use case for this type is plain data structs that do not require specialized functions. If
for some reason you want to add functionality to these types, you will need to export standalone
functions that accept this type. **Note** that you will incur a translation cost from the binding
language to the Rust code on each call.

### Objects

[Objects](https://mozilla.github.io/uniffi-rs/latest/udl/interfaces.html), also known as Interfaces
in UDL terms, are opaque types that wrap a Rust type and can only be interacted with via a specific
set of member functions. In this case uniffi wraps the pointer to the object and exposes that to the
target language. This is akin to the following in C:

```c
struct Foo;

struct Foo* foo_new();
int foo_get_bar(struct Foo* this);
```

This type is better suited for Rust types you do not wish to or can not directly be translated to
the binding language.

Unlike Records, objects must be `Send + Sync`. This is enforced since there is no guarantee from
which thread this binding code will be called. If the type is mutable, a common a approach is to
wrap the type in a `RWLock`:

```rust

#[derive(uniffi::Object)]
pub struct MyType {
    lock:RWLock<MyRustType>,
}

#[uniffi::export]
impl MyType {
    pub fn read(&self) {
        let guard = self.lock.read();
    }

    pub fn write(&self) {
        let mut guard = self.lock.write();
    }
}
```

## Static Functions

Unfortunately as of today (2024/06/17), uniffi has no support for exporting static methods on types.
There is an [open ticket](https://github.com/mozilla/uniffi-rs/issues/1074) for this feature.

One can either add the method to an existing object or export as a standalone function.

## Traits/Callbacks

[Traits](https://mozilla.github.io/uniffi-rs/latest/udl/callback_interfaces.html) that are required
to be implemented by the binding language are also required to be `Send + Sync` compatible.

Contrary to objects, which need to be declared as `Arc<Object>`, traits are always of type `Box<dyn
Trait>`.

### Cyclic References

Unfortunately, when creating implementation of these traits in other languages it is possible to
introduce cyclic references. This is not a problem unique to uniffi, but is present in many other
C based API exports.

When writing your implementation, ensure that any instance members have a weak
reference to the data they need to act upon.

### uniffi::UnexpectedUniFFICallbackError

In previous versions of uniffi (<0.25) it was required that any error returned should derive a
`From` implementation for this type. This error is very rare and is only triggered if there is
some sort of "lifetime" issue with data referenced by the traits implemented in binding languages.

This requirement has been removed in newer versions and if this is not implemented, your code will
panic unexpectedly. We recommend adding a `From<uniffi::UnexpectedUniFFICallbackError>`
implementation to every error type you export to avoid surprises.

## Custom Types

While uniffi has support for [custom types](https://mozilla.github.io/uniffi-rs/latest/udl/custom_types.html)
they tend to work best if they are used in the same crate they are declared in.

While it is possible to use custom types from different crates, we have run into some issues that
can lead to compile errors in the binding language.

### Incorrect bindings

You may run into [this issue](https://github.com/mozilla/uniffi-rs/issues/2025) when trying to
import custom types from a dependency. The only way around this at the moment is to do the conversion
manually from the underlying to the new type in the exported function.

E.g.:

```rust
// Crate A;

pub struct MyType(pub u64);
uniffi::custom_new_type(MyType, u64);

// Crate B;

#[uniffi::export]
pub fn do(value:u64) {
    let value = MyType(value);
    // do something with value
}

```

### Missing Lift Implementations

If you define a custom new type in Crate A, but do not have anything that exports in Crate A you may
find that your project can error out with missing implementation details for that type.

It is unclear if this also affects the UDL version, but one quick way to resolve this is to simply
create a dummy struct which is exported.

E.g:

```rust
pub struct MyType(pub u64);
uniffi::custom_new_type(MyType, u64);

mod hidden {
    #[derive(uniffi::Record)]
    struct DummyExport {
        pub custom: MyType,
    }
}
```

### `Lift<UniFfiTag>` is not implemented for `MyType`

Even if you resolve the previous issue you can still run into
[this issue](https://github.com/mozilla/uniffi-rs/issues/2025). This can be resolved by [the
approach listed in the thread](https://github.com/mozilla/uniffi-rs/issues/1988#issuecomment-1936018497).

## Error Handling

Other than all the error variants needing to be `Send + Sync` compatible, [this
RFC](../../../documentation/docs/rfcs/0002-error-handling.md) also covers
some more details about things to be aware of when dealing with errors from uniffi.

## Async

Since v0.26 uniffi has full support for
[exposing async functions](https://mozilla.github.io/uniffi-rs/0.27/futures.html) in the first party
languages. Support in third party languages depends on the maintainer.

One thing to keep in mind is that uniffi uses `async-std` executor internally. This can be a problem
if your code depends on functionality only present in tokio.

To override the runtime use the following attribute with `uniffi::export`:

```rust
#[uniffi::export(async_runtime = "tokio")]
pub async fn my_async_function() {...}
```

While it is possible to override the async runtime for a function or method (the latter requires
v0.28 for all the cases) to use tokio, you can not configure this runtime and as such, it may not
support everything you need.

It is generally recommend that you manage your own `tokio::Runtime` for uniffi and use that to drive
the async features. When managing your own runtime, you can either expose a sync function which uses
`Runtime::block_on` or expose an async function which internally uses `Runtime::spwan` and then uses
the uniffi runtime to wait on the task.

```rust
static RUNTIME: tokio::Runtime = ...;

// Sync version
#[uniffi::export]
pub fn my_async_function() {
    RUNTIME.block_on(async {...})
}

// Async version
#[uniffi::export]
pub async fn my_async_function() {
    RUNTIME.spawn(async move {
        ...
    }).await
}
```

There are pros and cons to each approach. The sync version is the more straightforward version for
Rust code, but requires that the binding language deal with the fact that the function blocks. The
async version adds more complexity to the Rust code as the runtime may spawn the execution of the
async code in a different thread, but the binding language can correctly await on the result. The
latter may or may not be important in the context of UI applications.

## Garbage Collection

This section is mostly dedicated to languages which use garbage collection (Kotlin & Pyton). Swift
uses automatic reference counting and also has the concept of destructors, so it not affected.

### `destroy()`?

**UNDER NO CIRCUMSTANCE SHOULD YOU CALL THE `destroy()` FUNCTION DIRECTLY IN THE BINDING LANGUAGE -
HERE BE DRAGONS**.

This is a function uniffi uses to cleanup the rust side of things, calling this function with active
references to this exported type will lead to Armageddon.

### Manual Release

This is just a reminder to expose functions which manually release resources such as file handles,
channels, etc... via a dedicated function rather than the `Drop` implementation.

One can potentially rely on the Garbage collector to eventually release the type and cause the Rust
`Drop` implementation to run. Whether this is acceptable or not needs to be carefully considered.

For instance, one trick you can use to manually close a file handle is wrapping it in an `Option`.

```rust
// NOTE: for the sake of brevity, error handling best practices are ommited
// from this example.
#[derive(uniffi::Object)]
pub struct File {
   handle: Mutex<Option<std::fs::File>>
}

imp Drop for File {
    fn drop(&mut self) {
        let Some(file)= self.file.lock().take() {
            file.close().expect("failed to close");
        }
    }
}

#[unffi::export]
impl File {
    pub fn write(&self, bytes:&[u8]) {
        // This should never fail.
        self.handle.lock().unwrap().write(bytes).unwrap();
    }


    // Close the file handle. Type can no longer be used after wards.
    pub fn close(&self) {
        // This should never fail.
        let Some(file)=self.handle.lock().unwrap().take(); {
            file.close();
        }
    }
}
```

### Android Auto Close

By default the uniffi Kotlin requires that when you are finished using an export Object, you call
`close()` manually ([link](https://mozilla.github.io/uniffi-rs/latest/kotlin/lifetimes.html)).

This is of course error prone and tedious. However, there is a way to have the GC auto close these
types when the GC runs. If you add the following to `uniffi.toml`:

```toml
[bindings.kotlin]
android=true
android_cleaner=true
```

and pass this to the binding generator, the bindings generator will generate code that hooks up to
the Garbage collectors cleanup phase automatically calls this for you when no active references
remain to this the exported instance.

## Miscellaneous

This section contains some other issues that we have encountered that you may also run into.

### Naming Clashes

It is recommended to avoid exporting anything that can potentially collide with that binding's
language existing list of types.

For instance, it is pretty common in Rust to just have an error enum named `Error`, directly
exporting this type will cause compilation errors in Swift as it conflicts with the builtin `Error`
type.

### Missing Constructors in Kotlin

Sometimes it is possible to run into a case where the default constructor for an object type is not
generated correctly. This seems to occur mostly when this constructor is async.

E.g.:

```rust
#[derive(uniff::Object)]
struct Foo{}

#[uniffi::Export]
impl Foo {
    #[uniffi::constructor]
    pub async fn new()->Self{...}
}
```

A fix for this is to just rename `new()` with a different name.
