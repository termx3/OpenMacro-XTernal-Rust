# Contributing to OpenMacro XTernal

Thanks for your interest in contributing. To keep the project legally
defensible and to allow it to stay under a single, enforceable license, every
contribution is subject to the terms below.

## License of contributions

OpenMacro XTernal is licensed under the **GNU Affero General Public License,
version 3.0 only (AGPL-3.0-only)**. All contributions are accepted under the
**Contributor License Agreement (CLA)** in [`CLA.md`](CLA.md).

By opening a pull request, pushing a commit, or otherwise submitting any
contribution (code, documentation, assets, configuration, or other material),
**you agree to the CLA in full.** If you do not agree, do not submit a
contribution.

## Developer Certificate of Origin (sign-off)

Every commit must be signed off, certifying the
[Developer Certificate of Origin 1.1](https://developercertificate.org/):

```
git commit -s -m "your message"
```

## Ground rules

- Do not submit code you do not have the right to license (no copied code from
  closed-source or incompatibly licensed projects).
- Do not remove, alter, or obscure any copyright, license, SPDX, or attribution
  header in any file.
- New source files must carry the standard project header (see any file in
  `src/` for the template).
- Third-party code (crates declared in `Cargo.toml`, or anything under
  `vendor/` / `third_party/`) is third-party; do not relicense or restamp it.
