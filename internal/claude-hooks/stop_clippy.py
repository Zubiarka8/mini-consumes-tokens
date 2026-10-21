#!/usr/bin/env python3
"""Retired Clippy Stop hook; stale registrations may call this harmless no-op.

Clippy is an explicit project check and must not run automatically when an
agent session stops. Keeping this entry point prevents cached registrations
from running the old check until the client reloads its configuration.
"""
