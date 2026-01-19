# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Other

- FOUN-271 PEST parser CR fixes
- FOUN-271 PEST grammar query parser
- FOUN-259 index and search by emojis.
- Fix test result order
- Allow complex tags to be searched as well with prefix and eq
- FOUN-251 tag prefix search
- Simplify Expression tree building
- FOUN-268 matching token positions in wasm
- Tag NPM package as pre-release in release:web:dev
- Add WASM methods to construct the expression tree for js (rust generics, enums are an issue across the browser/wasm boundary)
- FOUN-194 Add export/import features for the engine
- Fix unmodified behavior as well
- Fix blob release in collection and text
- Add missing ge (>=) and le (<=) query functions
- Update README, hide cantor_pairing
- 0.5.0-preview-2
- FOUN-252 Separate stats and results so that scoring can be done over multiple engines
- FOUN-238 implement stateful engine reset
- Assert same trigram sort order as str
- Use the lean trigram
- Lean trigram
- FOUN-250 threshold options
- FOUN-248 Support highlighting of found terms
- Optimise entry insert performance with Write-Ahead log (WAL) approach
- FOUN-237 - New crux demo
- Add efficient storage backend options (fs/memory/file), batch write commits,...
- Prepare version 0.5.0-preview
- Cleanup cargo workspace
- Remove storage, tokio, async
- Introduce unique index IDs
- Remove getrandom dependency
- Remove legacies, settle on sans-io only
- FOUN-229 web demo with sans-io engine
- FOUN-226 New query execution for the sans-io engine
- FOUN-215 Implementing sans-io engine blobs cleanup
- FOUN-215 Sans-IO Reader and Writer share caches
- FOUN-215 Update CLI demo with sans-io engine with storage
- FOUN-124 Query parser
- FOUN-215 PoC SansIO Engine
- flattened-index-builder tool for fast bulk index creation resulting in nested serialised structure
- refactoring of experimental flattened index code used on 150k email investigation into benches to support writer/reader index proposal
- FOUN-212 read/write managers refactoring
- Update Rust crate lru to 0.16
- Foun 208 android demo perf test
- entryindex changes from u16 to u32
- Docs update
- Remove ValueIndex from public API
- Reduce update APIs
- Combine multiple attribute search entries into one
- SearchEntry without attribute
- Enable streaming entry IDs
- improved test for Writer uncommitted changes
- unit tests for ScoreMap merges
- remove unnecessary mut ref
- unit tests for Partition
- Remove unused SearchResultList
- Move more analysis code to debug module
- Update Rust crate lru to 0.15
- RDBMS  index intersection instead of universe based filtering
- remove unused deps
- FOUN-53 conditional text index analysis
- FOUN-205 Consolidate entry move improvement, deduplicate
- FOUN-204 NewTextIndex becomes just a TextIndex
- FOUN-53-testing-dataset-drive-email
- Foun 205 bulk move for text index
- FOUN-204 tidy up the workspace

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
