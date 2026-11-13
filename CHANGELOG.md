# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2026-11-13

### CI/CD

- **deps:** Bump serde_json to 1.0.154 and smallvec to 1.16.5 ([c90c87a](https://github.com/bigmars86/rustine/commit/c90c87a4714bdab38ef051f84e86620bc0cff542))
- Generate the changelog in the release commit and check semver ([3e6df07](https://github.com/bigmars86/rustine/commit/3e6df0736fdca6372bacb68b87e92b19959a0d74))
- **deps:** Bump pyo3 from 0.29.3 to 0.30.1 ([3ed551d](https://github.com/bigmars86/rustine/commit/3ed551dbc7bad5354e2d1ca08c03770a2e19dd50))
- Publish to crates.io with trusted publishing ([1d2fb7f](https://github.com/bigmars86/rustine/commit/1d2fb7f4f5efd0d600f342bc4fe80dd438a6eeaf))
- Publish the crate from the crates.io environment ([99f7977](https://github.com/bigmars86/rustine/commit/99f79775350c7903ae332d6f5a0806f760264e26))
- Pin ubuntu-26.04, drop cargo-audit and fix the codecov input ([e01f431](https://github.com/bigmars86/rustine/commit/e01f4310a491314eb0fb0ade00c29d1b51734291))

### Miscellaneous

- Remove the git-cliff pre-push hook ([8f506ae](https://github.com/bigmars86/rustine/commit/8f506aedb4a14a01c81cfec166db1317587b31ea))
- Fix the spacing in the generated changelog ([288a285](https://github.com/bigmars86/rustine/commit/288a28509613d830ac2848ac6a91d3fcd1269bf6))

## [1.0.0] - 2026-11-13

### Added

- Add preserve_case option to keep tag-name case in out.* paths ([89fa742](https://github.com/bigmars86/rustine/commit/89fa742714afd1acb909c15820dddc00f9d6697c))
- Make the match length limit adjustable with search_window ([a664e32](https://github.com/bigmars86/rustine/commit/a664e32d491da9d753d4b2852fa906f4a64b3c99))

### Fixed

- Keep regex search window and back-references on UTF-8 char boundaries ([eeee3e3](https://github.com/bigmars86/rustine/commit/eeee3e327cbaa5b1aa083915405640baecb17a68))
- Make StreamingRunner results serializable ([9b8b373](https://github.com/bigmars86/rustine/commit/9b8b373d5f21355346c0c25a89d63c6f3c860cd8))
- End the run on do.fail inside sub-grammars and exit rgel with status 1 ([09609e1](https://github.com/bigmars86/rustine/commit/09609e121cba7c09570b91ed0aa767eff8353bd6))
- Ignore out.create, out.add and out.replace with an empty path ([c480877](https://github.com/bigmars86/rustine/commit/c480877ba92da765f34bec4db0ba82aa492f088c))
- Keep the cursor in place for out.open, out.enter and out.add_attribute with an empty path ([c45a245](https://github.com/bigmars86/rustine/commit/c45a245a5fa5dd024001ddd380472eb27f0775fa))
- Import the Linux Python wheel without a static TLS error ([ae700b6](https://github.com/bigmars86/rustine/commit/ae700b6ad4b31ef2a54fca237f1fa0444f03a7d9))
- Validate the grammar in run_grammar, run_grammar_xml and run_grammar_yaml ([83d245c](https://github.com/bigmars86/rustine/commit/83d245cdd7f5c937c6e574e4ceda142385162bdb))
- Reject encodings other than UTF-8 instead of ignoring them ([9a71d91](https://github.com/bigmars86/rustine/commit/9a71d9148740e769d5184a9af788a255d489feec))
- Report non-ASCII lexer errors without panicking ([9d1cee8](https://github.com/bigmars86/rustine/commit/9d1cee80654cb966eb2f3508ee097a02597db32f))
- Skip fast paths step over whole characters and leave Unicode classes to the regex ([1eba79f](https://github.com/bigmars86/rustine/commit/1eba79f484f1a6ff65b7eedbeb206793b12748c1))
- Write YAML that loads as the same data as the JSON output ([8ddaa2a](https://github.com/bigmars86/rustine/commit/8ddaa2a75f6ab265f3b7925c233a23770e5c6fe5))
- Report input the entry grammar leaves unconsumed as an error ([68ce565](https://github.com/bigmars86/rustine/commit/68ce56501cc372d8bb19ecdaeb7bcbe47bd858ed))
- Limit grammar calls to 250 nested levels instead of overflowing the stack ([2fef470](https://github.com/bigmars86/rustine/commit/2fef47010ea8046720ae22127a43fa98752d34a9))
- Identify nodes like Gelatin, so add_attribute no longer splits a node ([992d8cf](https://github.com/bigmars86/rustine/commit/992d8cf9ef9d5e895ae7935cee02b5c413144e66))
- Raise ValueError from parse_to_json and drop the XML and YAML AST dumps (**BREAKING**) ([50d0dc5](https://github.com/bigmars86/rustine/commit/50d0dc503fccb9155bcb8ee145a6675fe9b1db35))
- Never write ill-formed XML (**BREAKING**) ([d2c3425](https://github.com/bigmars86/rustine/commit/d2c342529a2f83a1f9916c53290fb17c8ec75023))

### Performance

- Track token line/column incrementally in the lexer ([e62cafe](https://github.com/bigmars86/rustine/commit/e62cafedcaeb0b1d975b50e9343b16b6bceadd86))
- Compile identical field-list regexes only once ([2f17725](https://github.com/bigmars86/rustine/commit/2f177256f549a72d6e29e54e307ec6d3adfd65f8))
- Skip per-match metadata when only the output tree is needed ([805e6a2](https://github.com/bigmars86/rustine/commit/805e6a24c02e49afc75ad5e11b621a13802ac04b))
- Share cached path segments and enter new nodes only for on_add triggers ([d815a99](https://github.com/bigmars86/rustine/commit/d815a99349d0bb87e4bb05556e6173486a9849a2))
- Bound the path cache instead of keeping every unique path ([5337f3b](https://github.com/bigmars86/rustine/commit/5337f3b77b384a73f17cf3d64ceb7930cb265892))
- Hash output child keys once and only for indexed parents ([d4618d7](https://github.com/bigmars86/rustine/commit/d4618d7e490e6a6232124a621035ea93ca7ff312))
- Parse out.* paths with fewer temporary strings ([635998f](https://github.com/bigmars86/rustine/commit/635998f5e676d72c894adad3de768d8d5c68a661))
- Interpolate captures into action arguments without temporary strings ([9cbd096](https://github.com/bigmars86/rustine/commit/9cbd0969c89bb1d58aa6ab86f6e2d57a6a387b1f))
- Escape JSON and XML output in runs instead of per character ([77425a6](https://github.com/bigmars86/rustine/commit/77425a625b790b0b2fe0fcc289e6d29988c73ef8))
- Match combined field lists with a single regex search ([76d464f](https://github.com/bigmars86/rustine/commit/76d464f47c9b9e3cb2f0cdad11c251becaa40039))
- Parse out.* paths on each call instead of caching them ([472dc53](https://github.com/bigmars86/rustine/commit/472dc53fa7c1b722b861d7194c764d747d1c42e4))
- Release the GIL while a grammar runs ([d1d0ab0](https://github.com/bigmars86/rustine/commit/d1d0ab0915c534519afbf718753e1f1b5fe8ac98))
- Stream rgel output to stdout instead of building it in memory first ([1c89bf5](https://github.com/bigmars86/rustine/commit/1c89bf54e9e1812241ac610eaf8882370c5ca2d5))
- Build the musl wheels with jemalloc and the Windows wheels with mimalloc ([f1da425](https://github.com/bigmars86/rustine/commit/f1da42549ccee0d54a96a21585a3dea300f9b0ec))

### Changed

- Fix clippy 1.99 lints ([2b7094b](https://github.com/bigmars86/rustine/commit/2b7094b43923e8f8c095e97bc018dbda60eefb81))
- Remove dead code and unused deps, simplify comments and docs (**BREAKING**) ([1a373df](https://github.com/bigmars86/rustine/commit/1a373df8439acbda51a982f84c74654ae33d64f0))
- Remove the streaming runner and lexer (**BREAKING**) ([7171dc1](https://github.com/bigmars86/rustine/commit/7171dc12810fcafded5436816dddc8b96f6c3852))
- One JSON and one YAML writer for strings and io::Write ([5b517a2](https://github.com/bigmars86/rustine/commit/5b517a24359fc39177c9153524e78f5cf71d1820))
- Make the extensible public types non_exhaustive (**BREAKING**) ([fb88431](https://github.com/bigmars86/rustine/commit/fb88431ed1a93a02a9a217f99de7b8665f752340))

### Documentation

- Update benchmark results ([4602661](https://github.com/bigmars86/rustine/commit/46026618fee014c6ff950a79291e9478d652579b))
- Measure Python Gelatin and the 2026-03 release on the same machine ([b44d14b](https://github.com/bigmars86/rustine/commit/b44d14b5359c7bd331932b0598859c69507cea6b))
- Report benchmarks from one comparison with instructions, memory and time ([26389f7](https://github.com/bigmars86/rustine/commit/26389f7f3fd21d2d91b1d5de39d1d1e11b9ccde9))
- Measure on CPython 3.11 and compare the wheel on Python 3.11 to 3.15 ([2d533dc](https://github.com/bigmars86/rustine/commit/2d533dc1a262fe3f87e6dd585432ab548d389d0d))
- Benchmark the shipped IOS XR config, with Python Gelatin 3.10 for reference ([8162bc0](https://github.com/bigmars86/rustine/commit/8162bc0613ae5b3bcd54bdd75177e75909240622))
- Support the latest release in SECURITY.md instead of a version table ([42b8b2a](https://github.com/bigmars86/rustine/commit/42b8b2ac9bb700c307a6bff71f0b9e7a3594e2b8))
- Note that one regex match covers at most 1000 bytes ([c099683](https://github.com/bigmars86/rustine/commit/c0996830ecd08480c7eac5820c455b8e149ad4b1))
- List where Rustine decodes out.* paths differently from Gelatin ([c7d52fe](https://github.com/bigmars86/rustine/commit/c7d52fe9178afd5a76043bc415988f7c7c0ef31d))
- Fix the Changelog heading and the links on Read the Docs ([c1a299e](https://github.com/bigmars86/rustine/commit/c1a299ef6172de4351d0abfe1be8f13b42f0518b))

### Testing

- Compare Rustine and Python Gelatin by instructions, memory and time ([a474744](https://github.com/bigmars86/rustine/commit/a4747441d36a3fc3e8b974902cd4c446112a6242))
- Ship the pseudonymized IOS XR config as a benchmark workload ([be9bf96](https://github.com/bigmars86/rustine/commit/be9bf961bb846d16ce5a1dcdbc8fd0d3870ca343))
- Run the compared engines in turns, with several Rust builds ([cd24024](https://github.com/bigmars86/rustine/commit/cd24024ce15c5ddb48d69e0b4086e96885feb741))

### CI/CD

- **deps:** Apply pending Dependabot updates and raise MSRV to 1.86 ([fc6c34a](https://github.com/bigmars86/rustine/commit/fc6c34aa7931d01a807463c9b8ed50e66fa15704))
- Skip perf_budget tests in PGO profile collection ([82a1ea6](https://github.com/bigmars86/rustine/commit/82a1ea63e45f6dcce9f3a820cd9072f2cb622f20))
- Require Python 3.11 and drop the end-of-life 3.9 and 3.10 (**BREAKING**) ([fa2a769](https://github.com/bigmars86/rustine/commit/fa2a769d3697f7ca352815d572e4a1a5fead1234))
- Test Python 3.13, 3.14 and 3.15 ([48d65a5](https://github.com/bigmars86/rustine/commit/48d65a514fb8f2ebb13bd9de6bbe3abaf7d5e2f3))
- List the supported Python versions as classifiers ([1170cd4](https://github.com/bigmars86/rustine/commit/1170cd436c60a1d6c08b3d2a9d98bc3d68c71101))
- Test the built wheels before publishing, without LD_PRELOAD ([f2ba211](https://github.com/bigmars86/rustine/commit/f2ba211b7b3561ada51cd36327881b5807f3cf2b))
- Fail the release when publishing the crate fails ([fd8ecd5](https://github.com/bigmars86/rustine/commit/fd8ecd539cbb2107bc59f8c57bb5d1e90e2905cd))
- Pin actions to commit SHAs and grant write access only where needed ([52bbac3](https://github.com/bigmars86/rustine/commit/52bbac368f7619ea4a5243be40372c9b14c5e61d))
- Run the fuzz targets for 60 seconds each and fix the execute harness ([1f19617](https://github.com/bigmars86/rustine/commit/1f196173568f59b60a46573172328fb8d1a235e3))
- **deps:** Update mimalloc to 0.1.52 and build its v3 allocator ([5fbcd3b](https://github.com/bigmars86/rustine/commit/5fbcd3b12f983c999d75d6e0f57c4261f7305487))
- Build the Intel macOS wheel on macos-latest ([011d58f](https://github.com/bigmars86/rustine/commit/011d58f2221184c668aa7e0d582b8756c8e0f655))
- Drop the i686 Linux wheel (**BREAKING**) ([1197080](https://github.com/bigmars86/rustine/commit/1197080b1a881c998f3cd448d6a0c1c9c56d76a8))
- Let a manual Release run build and test without publishing ([4b35ad8](https://github.com/bigmars86/rustine/commit/4b35ad89a4d151b811b59b3b124bdce48305ba52))

## [0.2.0] - 2026-06-25

### Added

- Add workflow dispatch input for re-releasing tags and enhance publish steps ([d566974](https://github.com/bigmars86/rustine/commit/d56697449e112cbc3358f9c6c3ab5e315e24614e))
- Support identifiers starting with digits in lexer and add regression tests ([9ad67f2](https://github.com/bigmars86/rustine/commit/9ad67f2fb20dcaa0f4c47fb4832cd0facbe7afd7))

### Fixed

- Simplify token collection in collect_tokens function ([1153c4f](https://github.com/bigmars86/rustine/commit/1153c4f81ca36bbd464952e64b3c36669444d689))

### CI/CD

- **release:** Guard tag against version and derive wheel version from Cargo.toml ([4723828](https://github.com/bigmars86/rustine/commit/4723828d969345cee5f1f06498be51cc440d6198))
- **deps:** Bump actions/checkout from 6 to 7 ([2ffe122](https://github.com/bigmars86/rustine/commit/2ffe122490603e2f7b6524a5669a4270a86ff252))

### Miscellaneous

- Update CI and release workflows to use latest action versions; bump dependencies in Cargo.toml and requirements.txt ([0750f3e](https://github.com/bigmars86/rustine/commit/0750f3ed624899466c58fef93b43abdc7a849929))
- Update dependencies in Cargo.toml, Cargo.lock, and requirements.txt ([6b758f9](https://github.com/bigmars86/rustine/commit/6b758f99746ce9aaaadc698ebaacc1530c2f00dc))
- Update memmap2 to version 0.9.11 in Cargo.toml and Cargo.lock ([cc0d0a9](https://github.com/bigmars86/rustine/commit/cc0d0a97f7c1e66623075390c6a38b460a430017))

## [0.1.1] - 2026-03-24

### Added

- Add workflow step to publish crate to crates.io ([3afc41a](https://github.com/bigmars86/rustine/commit/3afc41ad70b873ca6fc51eb377efc6410c8974e5))

### Fixed

- Update README badges and correct performance metrics ([b122f09](https://github.com/bigmars86/rustine/commit/b122f09a331e110b86723eeeb35e74621f1e6a38))

## [0.1.0] - 2026-03-24

### Added

- Initial release of Rustine v0.1.0 ([3dd7cc3](https://github.com/bigmars86/rustine/commit/3dd7cc307a7b49132dfeff749b3c2ec627b301f6))
