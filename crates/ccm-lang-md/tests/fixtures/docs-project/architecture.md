# Architecture

## Overview

A short description of the system.

## Components

### Indexer

Walks the file tree and writes to SQLite. See [[Parser]] for the tree-sitter wrapper. #core

### Parser

Wraps tree-sitter for one language.

# Testing

## Unit Tests

Per-crate `tests/parse.rs`.
