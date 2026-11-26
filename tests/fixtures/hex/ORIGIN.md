# Origin of the `hex` package

## Source

- Crate: `hex` 0.4.3 (crates.io), "Encoding and decoding data into/from hexadecimal representation."
- Repository: <https://github.com/KokaKiwi/rust-hex>, commit `b2b4370b5bf021b98ee7adc92233e8de3f2de792` (from the crate's `.cargo_vcs_info.json`).
- Files read: `src/lib.rs`, `src/error.rs` (and `src/serde.rs`, not ported).
- Licence of the origin: `MIT OR Apache-2.0`. This port uses it under the MIT licence. The origin's copyright notice is

  ```
  Copyright (c) 2013-2014 The Rust Project Developers.
  Copyright (c) 2015-2020 The rust-hex Developers.
  ```

  and its two licence texts are kept unchanged in [`LICENSE-MIT-origin`](LICENSE-MIT-origin) and [`LICENSE-APACHE-origin`](LICENSE-APACHE-origin). The port itself is under the repository's MIT licence.

## What was ported

| Origin (Rust) | Port (BMB, `src/hex.bmb`) | Contract |
| --- | --- | --- |
| `encode<T: AsRef<[u8]>>(data: T) -> String` | `hex::encode(data: &[u8]) -> String` | exact: `encoded` (bytes `2i`, `2i+1` are the lower-case digits of the high and low half of byte `i`), the result is all digits, and it decodes back to `data` |
| `encode_upper` | `hex::encode_upper` | the same, upper case |
| `decode<T: AsRef<[u8]>>(data: T) -> Result<Vec<u8>, FromHexError>` | `hex::decode(data: &[u8]) -> Result<Vec<u8>, FromHexError>` | exact: `Ok(v)` iff every byte is a digit and the length is even, with `v[k]` the value of digits `2k`, `2k+1`; else `OddLength` for an odd length; else `InvalidHexCharacter(c, i)` with `i` the first byte that is not a digit and `c` that byte |
| `decode_to_slice(data, out: &mut [u8])` | `hex::decode_to_slice(data: &[u8], out: &mut [u8])` | exact on the result: the errors in the origin's order `OddLength`, `InvalidStringLength`, `InvalidHexCharacter`; on `Ok`, `out` holds the decoding |
| `encode_to_slice(input, output: &mut [u8])` | `hex::encode_to_slice(input: &[u8], output: &mut [u8])` | exact: `Ok` and the lower-case encoding in `output`, or `InvalidStringLength` with `output` unchanged |
| `FromHexError` (`InvalidHexCharacter { c: char, index: usize }`, `OddLength`, `InvalidStringLength`) | `hex::FromHexError` (`InvalidHexCharacter(u8, usize)`, `OddLength`, `InvalidStringLength`) | (a type) |
| `impl Display for FromHexError` | `hex::message(e) -> String` | none (see below); tested against Rust |
| `#[derive(Debug)]` on `FromHexError` | `hex::debug(e) -> String` | none; tested against Rust |
| `fn val(c: u8, idx: usize)` | `hex::val` | exact: `Ok(value)` for a digit, else `InvalidHexCharacter(c, idx)` |
| `fn byte2hex`, `HEX_CHARS_LOWER`, `HEX_CHARS_UPPER` | `hex::hex_digit`, `hex::hex_char`, the same two constants | exact: the digit, and it reads back with `val` |
| (none) | `hex::round_trip(data)` | `decode(encode(data)) == Ok(data)`, proved from the contracts of `encode` and `decode` alone |

The proof gate passes on the package with no `assume`, no `#[external_body]` and no unproved termination: `bmb verify src/hex.bmb` proves the 13 definitions of the unit (110 obligations; 37 definitions with the `std::num` unit it uses) in about 2.4 s on the machine of the port (Z3 4.15.4; total rlimit of the unit 1,868,066). The largest is `encode_to_string` (18 obligations, rlimit 1,210,294, the largest single query 137,575). `bmb stability` (seeds 0 to 4) finds no definition whose verdict depends on the seed (largest rlimit spread 2.28x).

## What was changed, and why

- **One unit.** The origin's private module `error` (re-exported as `hex::FromHexError`) is folded into the unit `hex`, so callers write `hex::FromHexError` as in Rust.
- **`T: AsRef<[u8]>` parameters are `&[u8]`.** BMB has no `AsRef`. A caller with text passes `s.as_bytes()`.
- **`InvalidHexCharacter` holds the byte, not a `char`, and its fields are positional.** BMB has no variants with named fields (E0233), and `byte as char` is not a conversion BMB has (E0205). The origin's `c` is always `byte as char`, so the byte carries the same information; `message` and `debug` print it as the char, as the origin does.
- **No iterators.** `decode` (the origin's `chunks(2).enumerate().map(..).collect()`) and the two slice functions (the origin's `iter_mut().enumerate()` and `iter().zip(generate_iter(..))`) are range `for` loops over the pair index. The private `generate_iter` and the iterator `BytesToHexChars` have no counterpart.
- **No tuples.** `byte2hex(byte, table) -> (u8, u8)` became `hex_digit(x, upper)`, called once per half.
- **Digits as characters.** A `String` grows by `char`, and BMB has no `u8 as char`: `hex_char` makes the character with `char::from_u32` and proves the `None` arm dead (`assert(false)` there is proved).
- **Precondition `fits(data)` on `encode` and `encode_upper`:** `data.len() <= 2^62`, so that the result's length `2 * data.len()` fits. Rust panics (capacity overflow) for such inputs; BMB makes the caller prove it instead.
- **`Display` and `Debug` are functions** (`message`, `debug`): BMB has no standard-library traits. `push_debug_char` follows `char::escape_debug` for the 256 characters a byte can be: `\0 \t \n \r \' \\`, `\u{..}` for the controls, U+007F to U+00A0 and U+00AD, the character itself otherwise. This table was checked against rustc 1.97.0 (`tests/reference.rs`); the Rust escaping of U+00A0 and U+00AD depends on the Unicode tables of the Rust version.

## What was left out

- `serde` support (`src/serde.rs`, the `serde` feature): out of scope.
- The `FromHex` implementations for `[u8; N]` (the `from_hex_array_impl!` macro): out of scope; `decode_to_slice` into an array of that length is the same operation, and the tests use it that way.
- The traits `ToHex` and `FromHex` (generic over `FromIterator<char>`, with an associated type) and `impl std::error::Error`: BMB has no standard-library traits, no associated types and no blanket impls. `encode`, `encode_upper` and `decode` are what they forward to.
- The `no_std`/`alloc`/`std` features: there is one configuration, the origin's default.

## Contracts wanted and not stated

- `message`, `debug` and `push_debug_char` have no functional contract: a short exact one does not exist (it would restate the escape table and the decimal text of the index). They are tested instead, against Rust's own `Display`/`Debug` text for every byte value.
- `decode_to_slice`: on `InvalidHexCharacter`, the origin has already written the bytes of `out` before the failing pair. The contract says nothing about `out` on that error (it is exact on the result and on `out` for `Ok`). Stating it is possible but not short.
- `encode_to_string`'s capacity (`String::with_capacity(source.len() * 2)`) is not observable in the contract; `bmb lint` reports that `* 2` could be `+ 2` (W0904). That is expected: the capacity is a hint.

No contract that was wanted had to be weakened or dropped for a failed proof. Two proofs needed help that is worth recording:

- `round_trip`: from the two contracts the result cannot be an `Err`, but the solver did not see it. With the body `decode(encode(data).as_bytes())` alone and the final `#[uses(failed, first_bad, is_hex)]`, `bmb verify` reports E0811 for `back` ("a model exists for the instantiated definitions only", unconfirmed counterexample `data = []`, rlimit 122,643); with a shorter `#[uses]` it was E0802 with the same counterexample, which is not a real one. A `proof { match d { Ok(v) => {} Err(e) => match e { .. } } }` block, a case split with empty arms, makes it verify (2 obligations, rlimit 16,290).
- `decode` and `decode_to_slice` need `#[uses(first_bad, val_of, ..)]` beyond their default context; `bmb uses` found the lists.

## Tests

`tests/run.sh` builds `tests/driver.bmb` with `src/hex.bmb` (`bmb build`, which proves every obligation before it emits code; clang and llc are required) and runs each case of the two data files through the executable, comparing its standard output byte for byte:

- `tests/cases.txt`: the origin's tests (`src/lib.rs` `mod test`, `src/error.rs` `mod tests`), the examples of the origin's documentation and the RFC 4648 §10 BASE16 vectors, 47 cases.
- `tests/reference.txt`: 753 cases over every byte value (encode, encode_upper, decode both cases, round trip, both slice functions, and the `Debug` and `Display` text of the error for every byte that is not a digit: Debug at index 0 and 1, Display at index 0), with the expected results computed by the Rust standard library (`tests/reference.rs`), not by the port.

Result: 800 passed, 0 failed. Not exercised: the origin's `test_gen_iter` (it tests the private iterator `generate_iter`, which has no counterpart); inputs longer than the driver's 2^20-byte output buffer or near the `fits` bound; allocation failure. The proof covers every input, the tests cover the listed ones.

## The rewriting tool on this crate

`tools/rust2bmb` was run on the origin's `src/lib.rs` and `src/error.rs` (`rust2bmb src/lib.rs=hex src/error.rs`). The numbers and the counted list of what it reported are in [`tools/rust2bmb/README.md`](../../tools/rust2bmb/README.md#measured-on-hex-043). In short: of 37 items, 18 were written (5 with nothing reported) and 19 dropped and reported; 66 constructs were reported and 77 rewrites applied; the raw output does not pass `bmb check` (30 diagnostics in `hex`; the `error` unit alone checks). `src/hex.bmb` was written by hand from the origin, not by editing the tool's output: of its 339 non-blank lines, 43 are identical to a line of the tool's output, 33 of those braces or `Ok(())`, and 10 substantive (the `mod` line, three length checks, and the six-line `if` chain of `val` the tool made from the origin's `match`). The rest is 106 lines of `spec fn`, 20 contract clauses, 8 lines of proof (`assert`, `proof` blocks), 40 `#[summary]`, 13 `#[effects]`, 7 `#[uses]` and 145 other lines of code.
