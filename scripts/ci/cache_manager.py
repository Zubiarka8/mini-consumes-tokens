"""Cache key inputs and the runner-architecture check.

`Swatinem/rust-cache` already hashes rustc, Cargo.lock, every Cargo.toml and
the CARGO_*/RUST* environment. What it cannot know is the target triple a job
builds for, so two jobs on the same OS image but different targets could
share a cache. `key` adds the target, the host arch and a hash of the build
configuration files; `verify_arch` fails a job that landed on a runner of the
wrong architecture before it reuses or writes a cache.
"""

from __future__ import annotations

import hashlib
import os
from pathlib import Path

_TARGET_ARCH = {"x86_64": "X64", "aarch64": "ARM64"}

# Files whose content changes the compiled output beyond Cargo.lock.
_CONFIG_GLOBS = (
    "Cargo.lock",
    "Cargo.toml",
    "crates/*/Cargo.toml",
    "crates/*/build.rs",
    "rust-toolchain",
    "rust-toolchain.toml",
    ".cargo/config",
    ".cargo/config.toml",
)

# Environment that changes the compiled output.
_ENV_KEYS = ("RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_PROFILE_DEV_DEBUG", "CARGO_BUILD_TARGET")


class ArchMismatch(Exception):
    """The runner's architecture does not match the native target."""


def config_hash(root: Path, env: dict[str, str] | None = None) -> str:
    env = os.environ if env is None else env
    digest = hashlib.sha256()
    for pattern in _CONFIG_GLOBS:
        for path in sorted(root.glob(pattern)):
            digest.update(path.relative_to(root).as_posix().encode())
            digest.update(b"\0")
            digest.update(path.read_bytes())
            digest.update(b"\0")
    for key in _ENV_KEYS:
        digest.update(f"{key}={env.get(key, '')}\0".encode())
    return digest.hexdigest()[:16]


def key(root: Path, target: str, runner_os: str, runner_arch: str, profile: str = "dev",
        features: str = "default", env: dict[str, str] | None = None) -> str:
    parts = [runner_os, runner_arch, target, profile, features, config_hash(root, env)]
    return "-".join(p.lower().replace(" ", "_") for p in parts)


def target_arch(target: str) -> str:
    try:
        return _TARGET_ARCH[target.split("-", 1)[0]]
    except KeyError as err:
        raise ArchMismatch(f"unknown architecture in target {target}") from err


def verify_arch(target: str, runner_arch: str, mode: str = "native") -> None:
    """A native job must run on a runner of the target's architecture."""
    if mode != "native":
        return
    expected = target_arch(target)
    if runner_arch.upper() != expected:
        raise ArchMismatch(
            f"target {target} needs a {expected} runner, this runner is {runner_arch}"
        )
