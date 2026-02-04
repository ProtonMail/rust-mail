# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.1.1](https://gitlab.protontech.ch/backend-team/foundation-team/search//compare/proton-foundation-search-v1.1.0...proton-foundation-search-v1.1.1) - 2026-02-04

Public release under GPLv3 (no changes)

## [1.1.0](https://gitlab.protontech.ch/backend-team/foundation-team/search//compare/proton-foundation-search-v1.0.0...proton-foundation-search-v1.1.0) - 2026-01-19

### Added

- FOUN-325 Configurable stop words in the processor. The engine processor has also been exposed so that app may use the same one for their pre-processing. Also for WASM.

### Other

- Eliminated unsafe upon feedback from Jeremy &co.

## [1.0.0](https://gitlab.protontech.ch/backend-team/foundation-team/search//compare/proton-foundation-search-v0.4.0...proton-foundation-search-v1.0.0) - 2026-01-08

### Changed

Pretty much everything, we've been busy.

- Eliminated IO and async from the lib. These tasks are delegated to the app through events. The search, write, cleanup and export operations are state machines moved by an iterator producing the events.
- Eliminated partitioning as the splits did not scale with inserts. A new way of spliting large text index (not the whole engine) came in the form of buckets inside the text index. Bucket size can be configured, the default of 0 creates a bucket for each write transaction.
- Tuned index structures, algorithms, serialization... to get the best of search and insert performance while keeping the blobs slim.
- Thresholds (max distance, min similarity) have been moved into a new flexible query options struct.
- Merged all the crates into one to simplify release and avoid dependency hell.
- Moved support for WASM into the library itself gated behind a feature flag.
- Simplified the Processor trait to make it easier for apps to implement custom text extraction/normalization strategies.

### Added

- Wildcard search and exists search (`attr=*`)
- Query parser and query execution with support for nested logical conditions.
- Matched value and token position reporting in results (for highlighting).
- Delegated caching to the app so it can implement a suitable caching strategy.
- New Crux based demo where the sans-io engine lives in the core app and blob storage is a shell concern.
- Export - which can be used to migrate data or to bootstrap a new/dormant client through the server.

### Removed

- All forms of storage (encrypted or not)
- Support for streaming. The engine was found to be fast enough for applicable use cases and streaming is a complex matter facing nested logical conditions and the need to sort results by relevance.

### Other

- Tried hard to cut back dependencies. This involves a custom mem sharing primitive `Slot` to avoid the need for chanels and/or arc-swap. We are pretty close to a `no-std` lib.
- Beginning with this release we are going to support older release blobs for smooth upgrades.

## [0.4.0](https://gitlab.protontech.ch/foundation-team/search/compare/proton-foundation-search-v0.3.0...proton-foundation-search-v0.4.0) - 2025-03-07

### Added

- implement timing for search
- make sure write access is blocked when reading
- create a function to find if document is indexed

### Other

- create storage crate
- extract cipher module
- extract file format as parameter
- change visibility
- move text index search capabilities to view
- remove has_changed from every index
- split cache in multiple modules
- add method and struct wrapper documentation

## [0.3.0](https://gitlab.protontech.ch/foundation-team/search/compare/proton-foundation-search-v0.2.1...proton-foundation-search-v0.3.0) - 2025-02-28

### Added

- expose callback types on web

## [0.2.1](https://gitlab.protontech.ch/foundation-team/search/compare/proton-foundation-search-v0.2.0...proton-foundation-search-v0.2.1) - 2025-02-27

### Fixed

- global manifest should be default if not defined

## [0.2.0](https://gitlab.protontech.ch/foundation-team/search/compare/proton-foundation-search-v0.1.0...proton-foundation-search-v0.2.0) - 2025-02-26

### Added

- add a defragment function
- add function to clean engine directory
- add global manifest file and commit message

### Fixed

- *(core)* search result list handles better duplicate scores

## [0.1.0](https://gitlab.protontech.ch/foundation-team/search/releases/tag/proton-foundation-search-v0.1.0) - 2025-02-25

### Added

- add function to reset engine
- add operation to set attribute by value index
- change the partition file name when it gets updated
- allow to remove a attribute by index
- provide a way to insert a new attribute value
- provide a way to cancel the search
- create a function to return a list of elements
- create statistics structure
- add conditional queries
- complete wasm package binding
- prepare web package
- create tag index
- make sure user cannot add more than 256 values in attribute
- use distance crate to compute levenshtein distance
- allow to override the stemming language when inserting or searching
- forward language to processor
- add language in document structure
- writer will write in a writer directory
- add text search accross all attributes
- use term position in search
- make partition cache and size configurable
- adding logs
- implement how the partition gets split
- split boolean index
- move entry from integer index
- moving entries in text index
- add function to estimate partition file size
- wrap trigram and trienode in cachecell
- implement search text with score
- compute score for fuzzy search
- add way to search with prefix
- search by equal match in text index
- add filter function on boolean index
- add filtering function on integer index
- add partition cache
- add method to remove document from collection
- remove entry from text index
- wire to insert in integer and boolean indexes
- create boolean index
- create integer index
- can index and search
- create basic text index
- add schema in partition manager
- wire engine to be able to search and insert, missing logic inside
- propagate encryption layer to writer
- introduce partition parts
- prepare partition module
- move storage struct in engine
- implement a simple crypto trait
- export filesystem functions
- create basic functions for the stream search
- introduce query
- move processor definition to engine
- converts documents into entries based on schema
- create processor and writer
- add document entity
- create engine structure
- add way to save and load buckets
- allow to remove document
- create bases of index and search for engine
- create package for core library

### Fixed

- *(deps)* replace web-fs by browser-fs
- test the writer on web
- make sure splitting keep track of change
- remove lint allower
- implement Debug for processor
- please clippy and checks
- move readme
- properly format code
- apply clippy suggestions
- remove unused file

### Other

- *(pkg/core)* make publishable
- move published crates to workspace dependencies
- move index-prelude crate to workspace dependencies
- reference proton registry
- create script for releasing web npm package
- create web example
- add documentation on root modules
- move shared dependencies in workspace
- fix a few typos
- fix a few typos
- split prelude module
- move index operations in index module
- update writer doc
- writer returns custom error when adding document
- create generic add document function
- remove unused code
- rename partition module to index
- move text index in new crate
- move collection to separate crate
- move text index function in search trait
- move filtering in a trait
- rename trait
- use trait for indexes
- remove new function on EntryIndex
- add some documentation in code
- add boolean token
- move integer in integer inex
- rename token index
- move and rename positions
- rename position in boolean index
- add an attribute value index
- wrap document value into IndexedValue
- use with methods for accessing file content
- cached file are now independent
- remove unused code
- save and load with callbacks
- remove unused storage in manager
- writer writes operation index in separate file
- put long e2e tests under heavy-e2e feature flag
- move levenstein computation
- remove use of AsRef
- convert EntryIndex and AttributeIndex to structs
- move filters in respective index modules
- move processor in index module
- make sure smaller partition makes it stable [UNSTABLE]
- rewrite BucketSender with BTreeSet
- make sure search list bucket is sorted
- move move_entry and remove_entry in trait
- remove before_save method
- address clippy suggestions
- disable clippy warning for module inception
- disable warning for large error on PartitionFile trait
- remove lifetime on TrieNodeTermIterator
- please clippy suggestion
- move trigrams and trienodes in refcell
- move metadata into indexes
- make sure we can filter with integer and boolean filters
- update boolean index to filter on attribute first
- clear serializer for TextIndex
- add reverse index in collection
- make sure removing a doc keeps relations
- add some comments for the document removal process
- remove commented code
- use binary heap for list results
- move cache to partition level
- move logic in partition to avoid race condition [wip]
- check that we can remove a document
- remove unused methods
- collection should update metadata on insert
- make sure integer and boolean indexes can remove entries
- make sure text index can insert
- apply clippy suggestions
- add simple integration test
- add metadata to entry text attribute
- format codebase
- make sure the tests cover all mutants
- fix versions
- restart from basics
- move spellcheck usage inside index
- add benchmarks for indexing
- check that we can export and import engines and keep the scores
- simplify import
- remove useless reference counter
