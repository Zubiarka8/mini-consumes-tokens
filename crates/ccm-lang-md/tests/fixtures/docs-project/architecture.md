# Architecture

## Overview

A short description of the system.

## Components

### Indexer

Walks the file tree and writes to SQLite. See [[Parser]] for the tree-sitter wrapper. #core

See [[Parser#Testing]] for how it's tested, and [[Nonexistent Page]] for something that doesn't exist.

### Parser

Wraps tree-sitter for one language.

# Testing

## Unit Tests

Per-crate `tests/parse.rs`.
