# gotgan

The package manager of the BMB language. `gotgan` is to `bmb` what `cargo` is to `rustc`: the compiler works on files and workspaces, and `gotgan` works on packages. It reads a package's declaration (`package.json`), finds the package's files, and runs the compiler on them to check, prove, test and build the package. `gotgan` is itself written in BMB, and it is a package: it checks and proves itself.

**Status.** For now gotgan is working material for the language's own development and measurement. It is not yet declared usable by others, and its commands and declaration format will change. There is no registry, no fetching, and no dependency of one package on another yet: a package may use only the standard library units that the compiler carries.

## A package

```
<dir>/
  package.json      the declaration (below)
  src/<unit>.bmb    one file per unit, holding `mod <unit>`, in normal form (bmb fmt)
  tests/            test units (tests/<unit>.bmb) and case files (below)
  ORIGIN.md         where the package comes from, its licence and attribution
```

## The declaration

`package.json` is one JSON object. A member that is not listed here, a member of the wrong kind, a value that breaks a rule below, or a member name written with an escape or given twice is an error (exit code 2) that names it.

| Member | Required | Value |
| --- | --- | --- |
| `name` | yes | the package name: 1 to 64 bytes of lower-case ASCII letters, digits and `_`, the first a letter |
| `version` | yes | `MAJOR.MINOR.PATCH`, three runs of decimal digits |
| `summary` | yes | text: what the package is, in one sentence |
| `licence` | no | text: the licence of the package |
| `units` | yes | a non-empty array of distinct unit names. Each is a name as above that is not a reserved word of the language and not `std`; unit `u` is the file `src/u.bmb` |
| `deps` | no | an object with the one member `std`: an array of distinct units of the standard library (`base64`, `bytes`, `collections`, `crc32`, `intern`, `json`, `num`, `sha256`, `slice`, `str`, `utf8`). It must list exactly the standard library units the program reaches, the ones the compiler adds; `gotgan verify` checks this. Any other member of `deps` is an error: dependencies between packages are not part of the format yet |
| `origin` | no | an object describing where a ported package comes from (crate, version, repository, commit, licence). It is kept as written and not read |
| `bin` | no | an object mapping the name of each executable (a name as above) to its entry unit, the unit of `units` that holds its `fn main`. No two executables have the same entry unit. An executable is built from all the units but the entry units of the other executables |
| `tests` | no | an array of test suites, each an object with exactly the members `units`, `cases` and `format` (below) |

A test suite:

| Member | Value |
| --- | --- |
| `units` | a non-empty array of distinct test unit names (names that can name a unit, none of them a unit of the package); unit `t` is the file `tests/t.bmb`. The package's units and these are built together into one driver, an executable |
| `cases` | a non-empty array of distinct file names in `tests/` (1 to 64 bytes of ASCII letters, digits, `_`, `-` and `.`, the first not a dot) |
| `format` | `"args|stdin|stdout"`, the one case format so far |

What a valid declaration is, is stated in the source as the type invariant of `decl::Package` (`spec fn valid` in `src/decl.bmb`), and `decl::parse` is proved to give only values of that type, so what the two tables say about the name, the version, the units, the standard library units, the executables and the test suites is proved to hold of every declaration gotgan accepts. The kinds of `licence` and `origin` are checked but not part of that statement, and the match of `deps.std` with what the program reaches is checked by `gotgan verify`.

### The case format `args|stdin|stdout`

One case per line, `ARGS|STDIN|STDOUT`: `ARGS` are the arguments of the driver, separated by spaces or tabs; `STDIN` is what the driver reads on its standard input; `STDOUT` is what it must write on its standard output. The line is cut at its first two `|`, so `STDOUT` may hold `|`. `STDIN` and `STDOUT` are written with the escapes `\\` (a backslash), `\xHH` (the byte with the two hexadecimal digits `HH`), `\n`, `\r` and `\t`; any other backslash is an error. These are the escapes of `printf %b` that the files use, with the same meaning. Empty lines and lines that start with `#` are skipped. A case passes when the driver exits with 0 and writes exactly the expected bytes. Each case runs the driver once, with no shell in between.

The `hex` package of `bmb-packages` runs its tests with `tests/run.sh`. The same tests are declared by adding this member to its `package.json` (the copy in `tests/fixtures/hex` has it):

```json
  "tests": [
    {
      "units": ["driver"],
      "cases": ["cases.txt", "reference.txt"],
      "format": "args|stdin|stdout"
    }
  ],
```

## Commands

```
gotgan [--package DIR] check | verify | test | build | new NAME
```

The package is the directory `DIR` (default: the current directory). The compiler is the value of `BMB`, or `bmb` on `PATH`. Results are JSON objects, one per line, on standard output: the compiler's diagnostic rows as the compiler writes them, then one row for the command (`{"command":..,"package":..,..,"ok":..}`). Human-readable messages go to standard error. Exit codes: 0 the package passes, 1 it fails, 2 a usage error, a declaration that cannot be read or is not valid, or a case file that cannot be read or is not in its format.

- `gotgan check`: `bmb fmt --check` and `bmb check` on the package's units, and on the units together with the test units of each suite. Files are passed in the order of their unit names, the order in which `bmb fmt --check` compares several files. The row gives `units`, `test_units`, `fmt`, `check`.
- `gotgan verify`: `bmb verify` on the package's units, then `bmb query status` on them. It fails when a definition is not proved; when a definition, the standard library's included, has an escape hatch, a trust level `assumed` or `external`, or `"termination":"unproved"`; or when the standard library units the program reaches are not exactly `deps.std`. The row gives `defs` and `std_defs` (the definitions proved, of the package and of the standard library units it reaches), `obligations`, `rlimit` (the sum) and `rlimit_max` (the largest single query) over the package's definitions, `unproved`, `escapes`, `trusted`, `termination_unproved` (all four must be 0) and `std`.

  How the escape hatches are found: the rows of `bmb verify` carry `trust` only for a definition whose proof used an escape hatch, so an `assume` that the proof does not need leaves a row that says nothing (`tests/fixtures/escape`), and they do not carry `termination` either (`tests/fixtures/termination`). The rows of `bmb query status` carry, for every definition, `escapes` (the number of `assume` statements and `#[external_body]` attributes it holds, counted by the compiler on the syntax tree, not by matching text), `trust` and `termination`. The cost is a second proof run.
- `gotgan test`: for each test suite, `bmb build` of the package's units and the suite's test units into `DIR/target/gotgan/<first test unit>` (the build proves everything first), then each case of each case file, in order, run through that driver. A row for each case that fails (`file`, `line`, `args`, `status`, `expected`, `got` and `stderr`, the bytes written in the escapes above plus `\xHH` for every byte outside printable ASCII) and a row for each suite (`built`, `passed`, `failed`).
- `gotgan build`: `bmb build` of each executable of `bin` into `DIR/target/gotgan/<name>`, a row for each.
- `gotgan new NAME`: writes a new package `NAME` into the directory `NAME`, or `DIR` with `--package DIR`: `package.json`, `src/NAME.bmb` with one proved function, a test suite (`tests/driver.bmb`, `tests/cases.txt`) that passes, and an `ORIGIN.md` to fill in. It writes no file that exists, and refuses a name that cannot name a unit or is `driver` (the skeleton's test unit).

## Building and checking gotgan

gotgan is a package (`package.json` here, units in `src/`, the executable `gotgan` with the entry unit `main`). To build it the first time:

```
bmb build src/*.bmb -o gotgan        # the files in the order of their unit names
```

Building needs `clang` and `llc`, and proving needs `z3`. After that, `gotgan check`, `gotgan verify` and `gotgan build` work on it like on any package. `scripts/check.sh` builds gotgan with `$BMB build`, runs `check`, `verify` and `build` on it (the executable `gotgan build` makes must be the same bytes), `check`, `verify` and `test` on `tests/fixtures/hex`, every failing fixture of `tests/fixtures` with its expected exit code, and `gotgan new` followed by `check`, `verify` and `test` of the new package:

```
BMB=/path/to/bmb scripts/check.sh
```

## Licence

MIT (see [LICENSE](LICENSE)). `tests/fixtures/hex` is a copy of the `hex` package of `bmb-packages`, a port of the Rust crate `hex`; its `ORIGIN.md` and licence files say where it comes from.
