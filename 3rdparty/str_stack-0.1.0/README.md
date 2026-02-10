StrStack
========

Linux: [![Build Status](https://travis-ci.org/Stebalien/str_stack.svg?branch=master)](https://travis-ci.org/Stebalien/str_stack)

A string allocation library. This is primarily useful when you want to allocate
a bunch of small strings, use them, and then destroy them all together.

Documentation
-------------

https://stebalien.github.com/str_stack/str_stack/

Performance
-----------

* Allocation: ~2.5x speedup (for 1000 strings) (~42ns per string)
* Indexing: 0.73x speedup (slower) (~1.7ns per index)
* Iterate: 0.35x speedup (much slower) (~1ns per iteration)
