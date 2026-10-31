# Brief: serde-saphyr (serde YAML replacement)

Pure-Rust serde YAML deserializer, chosen to replace deprecated `serde_yaml@0.9` + `unsafe-libyaml` (RUSTSEC-2024-0449).

## Mental model

- Type-driven: parses YAML directly into `T: Deserialize`, no intermediate `Value` tree (unlike `serde_yaml`). No native `Value` type.
- Crate name `serde-saphyr`, import `serde_saphyr`. Entry points mirror `serde_yaml`: `from_str<T>`, `to_string<T>`, plus `from_str_with_options`, `from_reader`, `from_slice`, `from_multiple`.
- Parser: `granit-parser` (fork of `saphyr` pure-Rust parser). `deny unsafe`, fuzz-tested. No `unsafe-libyaml` / `libyaml-rs` C-transpiled code.
- MSRV 1.89 (we are on 1.85 — must bump `rust-version`).

## Non-obvious behaviors that bite

1. **No `Value`**: `serde_yaml::Value`, `Mapping`, `Number` don't exist. For abstract maps use `serde_json::Value` (supported via `deserialize_any`, source order preserved, keys must be strings). Our `validate_all` already deserializes into `serde_json::Value` — that path is trivial. `normalize_frontmatter` + `value_to_yaml_str` use `serde_yaml::Value` — must port to `serde_json::Value`.
2. **Borrowed `&str` only for verbatim scalars**: `from_str::<Data<'a>>` with `&str` fields fails if scalar needs folding/escape processing; use `String`/`Cow<str>`. We use owned `String` in `Task` — safe.
3. **Single-doc `from_str` rejects multi-doc**: `---`-delimited multi-doc returns `MultipleDocuments` error suggesting `from_multiple`. Our frontmatter is single-doc — safe, but error message changes.
4. **Non-finite floats rejected by default** (`.nan/.inf` error with location). `serde_json::Value` path converts passthrough to `Null`. Our frontmatter is strings only — safe. Use `NonFiniteFloatPolicy::AsString` only if needed.
5. **YAML 1.2-ish via pure parser, not libyaml 1.1**: merge keys `<<` supported natively (no `.apply_merge()` step unlike `serde_yaml_ng`). Edge-case schema differences (octal, `yes/no`) unlikely to hit `status/type/priority/area` string fields, but round-trip tests must cover.
6. **Error type**: single `serde_saphyr::Error` enum with `Location` (1-indexed row/col + byte span), `render()` with snippets. `Display` includes location — our `map_err(|e| format!("YAML parse error: {e}"))` keeps working, messages will change (update test assertions matching on message text).

## Failure modes / limits

- No lossless editing / comment preservation (we don't need it — we re-emit canonical frontmatter).
- Reader entry points require `DeserializeOwned` (no borrowing from `Read`). We use `from_str` on in-memory strings — unaffected.
- `to_string` output formatting differs from `serde_yaml::to_string` (we only use it as fallback for exotic values in `value_to_yaml_str`; canonical fields use `format!("{}: {}", key, str)` directly).

## Pointers

- Docs: `https://docs.rs/serde-saphyr/latest/serde_saphyr/fn.from_str.html` — `from_str`, borrowed-`str` example.
- Docs: `.../fn.to_string.html` — `to_string` example.
- Crate root: `.../serde_saphyr/` — `serde_json::Value` via `deserialize_any`, `NonFiniteFloatPolicy`, v2 `AliasError{error: Box<_>}` breaking pattern-match note.
- Migration context: oneyaml team "choosing a Rust YAML crate" (2026-10-26) — `yaml_serde` keeps libyaml-unsafe, `serde-saphyr` is pure-Rust typed-reads choice; `serde_yaml_ng` still `unsafe-libyaml` (does NOT fix RUSTSEC).

## Operational reality

- Drop-in Cargo: remove `serde_yaml="0.9"`, add `serde-saphyr="2"`. No `package=` rename trick (different API).
- Must bump `rust-version="1.85"` → `"1.89"` in `Cargo.toml`; verify `rust-toolchain.toml` stable is ≥1.89 (it is, Oct 2026).
- Verify: `cargo tree -i unsafe-libyaml` empty, `cargo deny check` green, `cargo test` green. Watch for `smallvec@2 alpha` transitive (noted upstream, harmless for CLI).
