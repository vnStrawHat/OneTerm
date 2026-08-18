#!/usr/bin/env python3
"""Verify OneTerm's machine-readable workspace dependency policy and crate versions."""

from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY_PATH = ROOT / "scripts" / "dependency-graph-policy.json"
ROOT_MANIFEST = ROOT / "Cargo.toml"


def fail(messages: list[str]) -> None:
    for message in messages:
        print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def normal_workspace_dependencies(package: dict, workspace_names: set[str]) -> set[str]:
    return {
        dependency["name"]
        for dependency in package["dependencies"]
        if dependency["name"] in workspace_names
        and dependency.get("kind") in (None, "normal")
    }


def main() -> None:
    policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
    manifest = tomllib.loads(ROOT_MANIFEST.read_text(encoding="utf-8"))
    declared_members = set(manifest["workspace"]["members"])
    expected_members = set(policy["workspace_members"])

    errors: list[str] = []
    missing_members = sorted(expected_members - declared_members)
    unexpected_members = sorted(declared_members - expected_members)
    if missing_members:
        errors.append(f"workspace members missing from Cargo.toml: {missing_members}")
    if unexpected_members:
        errors.append(f"workspace members missing from policy: {unexpected_members}")

    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=ROOT,
            text=True,
        )
    )
    packages = {
        package["name"]: package
        for package in metadata["packages"]
        if package["id"] in set(metadata["workspace_members"])
    }
    workspace_names = set(packages)

    expected_internal_dependencies = policy["internal_dependencies"]
    for package_name, expected in sorted(expected_internal_dependencies.items()):
        actual = sorted(normal_workspace_dependencies(packages[package_name], workspace_names))
        if actual != sorted(expected):
            errors.append(
                f"{package_name} internal dependencies drifted: "
                f"expected {sorted(expected)}, found {actual}"
            )

    for backend in policy["backends"]:
        dependants = sorted(
            name
            for name, package in packages.items()
            if backend in normal_workspace_dependencies(package, workspace_names)
        )
        if dependants != [policy["app_package"]]:
            errors.append(
                f"{backend} must be a normal dependency of only "
                f"{policy['app_package']}; found {dependants}"
            )

    forbidden_shell_dependencies = set(policy["backends"]) | set(policy["feature_packages"])
    shell_dependencies = normal_workspace_dependencies(
        packages[policy["shell_package"]], workspace_names
    )
    forbidden = sorted(shell_dependencies & forbidden_shell_dependencies)
    if forbidden:
        errors.append(
            f"{policy['shell_package']} must remain feature/backend agnostic; found {forbidden}"
        )

    feature_names = set(policy["feature_packages"])
    allowed_feature_dependencies = policy["allowed_feature_dependencies"]
    for feature in sorted(feature_names):
        dependencies = normal_workspace_dependencies(packages[feature], workspace_names)
        actual_cross_feature = dependencies & feature_names
        allowed = set(allowed_feature_dependencies.get(feature, []))
        unexpected = sorted(actual_cross_feature - allowed)
        missing = sorted(allowed - actual_cross_feature)
        if unexpected:
            errors.append(f"{feature} has forbidden feature dependencies: {unexpected}")
        if missing:
            errors.append(f"{feature} is missing documented feature dependencies: {missing}")
        backend_dependencies = sorted(dependencies & set(policy["backends"]))
        if backend_dependencies:
            errors.append(f"{feature} depends on backends: {backend_dependencies}")

    # `[workspace.package] version` is the single version source; every crate
    # inherits it.
    workspace_version = manifest["workspace"]["package"].get("version")
    for package_name, package in sorted(packages.items()):
        if package["version"] != workspace_version:
            errors.append(
                f"{package_name} is version {package['version']!r}, expected the "
                f"workspace version {workspace_version!r} (use version.workspace = true)"
            )

    for package_name in ("oneterm-core", "oneterm-terminal"):
        dependencies = normal_workspace_dependencies(packages[package_name], workspace_names)
        forbidden_ui = sorted(
            dependency
            for dependency in dependencies
            if dependency.startswith("oneterm-")
            and dependency not in {"oneterm-core"}
        )
        if package_name == "oneterm-core" and forbidden_ui:
            errors.append(f"oneterm-core must remain a leaf; found {forbidden_ui}")

    if errors:
        fail(errors)

    print(
        f"Dependency graph policy passed for {len(packages)} workspace packages "
        f"and {len(declared_members)} explicit members."
    )


if __name__ == "__main__":
    main()
