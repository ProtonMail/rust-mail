Search engine library providing full-text and value search.

## Features

- 🔍 **Full-text Search**: Support for text searching - exact, prefix and fuzzy
- 📊 **Multiple Data Types**: Text, integer, boolean, and tag field types
- 🌐 **WASM Support**: Works in browsers via WebAssembly (with limits)

## Quick Start

Add to your Cargo.toml:

```toml
[dependencies]
proton-foundation-search = "0.1.0"
```

## Example usage:

```rust
use proton_foundation_search::engine::*;
use proton_foundation_search::document::*;

# async fn example() -> std::io::Result<()> {
// Create engine
let engine = Engine::builder().build();

// Index documents
let mut writer = engine.write().expect("single writer");
writer
    .insert(
        Document::new("doc1")
            .with_attribute("title", Value::text("Hello World"))
            .with_attribute("creation", 12345),
    )
    .unwrap();

for event in writer.commit()
{
    todo!("handle events");
}

// Search
for event in engine
    .query()
    .with_expression("hello".parse().unwrap())
    .search()
{
    todo!("handle events");
}

# Ok(())
# }
```

## Architecture

The library is built around several key components:

- **Engine**: Main entry point for search and indexing operations
- **Writer**: Handles document insertions, updates and removals
- **Query**: Builds and executes search queries

The engine is schema-less. It can handle different value types for the same attribute.
One may for example add integer attribute folder=123 and again tag attribute folder="todos"

The application can extend the engine with its own indices by implementing these traits: 
 - `IndexStore`
 - `IndexSearch`
 - `IndexExport`

Such custom indices can be configured through the engine builder.

The application may also override the built-in input processing by implementing 
the `Proc` trait and passing that to the engine builder.

## Features Flags

- `default` - Default features
- `heavy-e2e` - Enable heavy end-to-end tests
- `wasm-bindgen` - Direct WebAssembly support
- `parser` - Include a query parser

## WASM target

See more details in [README.WASM](README.WASM.md)

There are two approaches to consume the library on a wasm target: 
  a) directly consuming the NPM package, and
  b) wrapping the library in your own WASM build

### Directly consume the proton-foundation-search NPM package

While this is most convenient, it also limits what one can do with the library
as the WASM-FFI interface is notoriously restrictive - no generics, no references in many places...

This approach will not allow you to fully customize/configure the engine.

The web demo in the project shows this approach.

It's good for a start with defaults. Power users, pick the next one.

### Wrapping the search lib in your own WASM build

This gives you full access to the RUST APIs unhindered by FFI.

The crux based demos show this approach.

In this case, depending on your app architecture, you may or may not need the wasm_bindgen decorations. So these can be opted in/out with the `wasm_bindgen` feature.

## Advanced Usage

### Complex Queries

If compiled with the `parser` feature, queries can be parsed from strings.
Applications will likely wish to programatically set some query filters though
and for that, a trip to string and parse would make no sense. So you may construct
query trees explicitly from the code as well.

```rust
# use proton_foundation_search::engine::*;
# use proton_foundation_search::document::*;
use proton_foundation_search::query::expression::{Expression, Func};
# 
# let engine = Engine::builder()
#       .build();
let query = engine.query()
    .with_expression(
        Expression::And(vec![
            Expression::attr("title", Func::Matches, Value::text("hello")),
            Expression::attr("body", Func::Matches, Value::text("world")),
        ])
    );
```

## Performance

The library uses several techniques for optimal performance:

- In-memory caching of frequently accessed partitions
- Optimized full-text search algorithms

## Security

- No unsafe code
- Input validation on all public APIs

## Building

```bash
# Regular build
cargo build --release

# Run tests
cargo test
# Run heavy end-to-end tests
cargo test --features heavy-e2e
```

## License

TBD
