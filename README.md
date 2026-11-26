# gotgan

The package manager of the BMB language. `gotgan` is to `bmb` what `cargo` is to `rustc`: the compiler works on files and workspaces, and `gotgan` works on packages. It reads a package's declaration (`package.json`), finds the package's files, and runs the compiler on them to check, prove, test and build the package. `gotgan` is itself written in BMB, and it is a package: it checks and proves itself.

**Status.** For now gotgan is working material for the language's own development and measurement. It is not yet declared usable by others, and its commands and declaration format will change. There is no registry and no fetching: a package may depend on other packages only by the path of their directory, with no version selection, and on the standard library units that the compiler carries.

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
| `units` | yes | a non-empty array of distinct unit names. Each is a name as above that is not a reserved word of the language and not `std`; unit `u` is the file `src/u.bmb`. In a package without `bin` (a library) every unit must be named after the package: `NAME` or `NAME_...` (see Dependencies) |
| `deps` | no | an object with the members `std` and `packages`, both optional. `std`: an array of distinct units of the standard library (`base64`, `bytes`, `collections`, `crc32`, `intern`, `json`, `num`, `sha256`, `slice`, `str`, `utf8`). `packages`: an object mapping the name of each package depended on (a name as above, not the package's own) to an object with the one member `path`, the non-empty path of that package's directory, relative to this package's directory (or absolute). What both must list is checked by `gotgan verify` (see Dependencies). Any other member of `deps` or of a dependency is an error |
| `origin` | no | an object describing where a ported package comes from (crate, version, repository, commit, licence). It is kept as written and not read |
| `bin` | no | an object with at most one member: the name of the package's executable (a name as above) and its entry unit, the unit of `units` that holds its `fn main`. At most one, because the compiler reads the units of a package as one program, and a program has one `fn main` (E0702). A package with `bin` cannot be a dependency |
| `tests` | no | an array of test suites, each an object with exactly the members `units`, `cases` and `format` (below) |

A test suite:

| Member | Value |
| --- | --- |
| `units` | a non-empty array of distinct test unit names (names that can name a unit, none of them a unit of the package); unit `t` is the file `tests/t.bmb`. The package's units (but the entry unit of its executable, which has its own `fn main`), its dependencies' units and these are built together into one program, the suite's driver |
| `cases` | a non-empty array of distinct file names in `tests/` (1 to 64 bytes of ASCII letters, digits, `_`, `-` and `.`, the first not a dot) |
| `format` | `"args|stdin|stdout"`, the one case format so far |

What a valid declaration is, is stated in the source as the type invariant of `decl::Package` (`spec fn valid` in `src/decl.bmb`), and `decl::parse` is proved to give only values of that type, so what the two tables say about the name, the version, the units and the naming rule of a library, the standard library units, the packages depended on (distinct names, none the package itself, non-empty paths), the executable and the test suites is proved to hold of every declaration gotgan accepts. The kinds of `licence` and `origin` are checked but not part of that statement, and what `deps` must list is checked by `gotgan verify`.

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

The package is the directory `DIR` (default: the current directory). Every command first reads the closure of the package's dependencies (see Dependencies) and passes the compiler the files of the package and of every package in the closure. The compiler is the value of `BMB`, or `bmb` on `PATH`. Results are JSON objects, one per line, on standard output: the compiler's diagnostic rows as the compiler writes them, then one row for the command (`{"command":..,"package":..,..,"ok":..}`). Human-readable messages go to standard error. When a diagnostic names an unknown `A::..` and `A` is a unit of no package of the closure, gotgan adds that a call to another package needs it in `deps.packages`. Exit codes: 0 the package passes, 1 it fails, 2 a usage error, a declaration (the package's or a dependency's) that cannot be read or is not valid, a closure that is not usable (below), or a case file that cannot be read or is not in its format.

- `gotgan check`: `bmb fmt --check` and `bmb check` on the files of the package and its closure, and on the files of each test suite's program. The row gives `units`, `dep_units`, `test_units`, `packages` (the closure but the package), `fmt`, `check`.
- `gotgan verify`: `bmb verify` on the files of the package and its closure, then `bmb query status` and `bmb reads` on them. It fails when a definition is not proved; when a definition anywhere in the closure, the standard library's included, has an escape hatch, a trust level `assumed` or `external`, or `"termination":"unproved"`; or when a package of the closure does not declare exactly what its units use (see Dependencies). The row gives `defs`, `dep_defs` and `std_defs` (the definitions proved, of the package, of the other packages of the closure and of the standard library), `obligations`, `rlimit` (the sum) and `rlimit_max` (the largest single query) over the package's own definitions, `unproved`, `escapes`, `trusted`, `termination_unproved` (all four must be 0), `declared`, `std` (the package's `deps.std`) and `packages`.

  How the escape hatches are found: the rows of `bmb verify` carry `trust` only for a definition whose proof used an escape hatch, so an `assume` that the proof does not need leaves a row that says nothing (`tests/fixtures/escape`), and they do not carry `termination` either (`tests/fixtures/termination`). The rows of `bmb query status` carry, for every definition, `escapes` (the number of `assume` statements and `#[external_body]` attributes it holds, counted by the compiler on the syntax tree, not by matching text), `trust` and `termination`. The cost is a second proof run.
- `gotgan test`: for each test suite, `bmb build` of the suite's program (above) into `DIR/target/gotgan/<first test unit>` (the build proves everything first), then each case of each case file, in order, run through that driver. A row for each case that fails (`file`, `line`, `args`, `status`, `expected`, `got` and `stderr`, the bytes written in the escapes above plus `\xHH` for every byte outside printable ASCII) and a row for each suite (`built`, `passed`, `failed`).
- `gotgan build`: `bmb build` of the files of the package and its closure into `DIR/target/gotgan/<name>` for the executable of `bin`.
- `gotgan new NAME`: writes a new package `NAME` into the directory `NAME`, or `DIR` with `--package DIR`: `package.json` (with `licence` and `origin` to fill in), `src/NAME.bmb` with one proved function, a test suite (`tests/driver.bmb`, which reads only its argument, and `tests/cases.txt`, whose header names its format) that passes, and an `ORIGIN.md` to fill in; all of it in normal form. It writes no file that exists, and refuses a name that cannot name a unit or is `driver` (the skeleton's test unit); for a name with `-` it suggests the spelling with `_`.

## Dependencies

A package calls another package's functions by their unit paths (`hex::encode(..)`), and declares it:

```json
  "deps": {
    "std": ["num"],
    "packages": {
      "hex": { "path": "../hex" }
    }
  },
```

No compiler support is involved: gotgan reads the declaration at each path, follows the dependencies of the dependencies (the closure), and passes the compiler the unit files of every package of the closure together with the package's own (never their test units). All the units of a dependency are visible to the caller. gotgan refuses (exit code 2), saying which package and why:

- a declaration in the closure that cannot be read or is not valid, or that is not named as the dependency that points at it;
- a dependency with `bin`: a package with an executable cannot be a dependency;
- a cycle of dependencies (found over the graph that has been read; `resolve::order` is proved to return, when it finds no cycle, a ranking under which every dependency edge goes down, which is a certificate that there is none);
- the same package reached at two different directories. Directories are compared after a lexical normalization (empty and `.` components dropped, `..` taking away the component before it), so `../twice` and `./../twice/` from the same directory are the same, but a symbolic link is not followed: there is no call in the system interface that gives the canonical path or the current directory;
- a unit name in two packages of the closure (`resolve::first_repeat`, proved to return none only when the names are distinct).

**The naming rule.** The units of a package and of all its dependencies share one namespace, so every unit of a library (a package without `bin`) must be named `NAME` or start with `NAME_`, where `NAME` is the package's name. It is part of the validity of a declaration, so every command refuses a library that breaks it, and a dependency that breaks it is refused where it is read. A package with `bin` is exempt for its own units (gotgan itself has units `decl`, `text`, ..) and can not be a dependency. With the rule and distinct package names, unit names are distinct, except for packages named `a` and `a_b`, where `a`'s unit `a_b` meets `a_b`'s: the clash check reports it.

**What `deps` must list, exactly.** `gotgan verify` reads, with `bmb reads`, the read set of every definition of the closure, and attributes each definition to its package by its unit. For each package `P` of the closure (the package itself and every dependency):

- `P`'s `deps.packages` must be exactly the packages, other than `P`, whose functions or types the definitions of `P`'s units read as callees or types. A package that `P` reaches only through another one is not declared by `P`; a package whose units `P` uses directly must be declared even when it is also reached through another (`tests/fixtures/deps/ta_skip`); a declared package that `P`'s units do not use is an error (`tests/fixtures/deps/unused`). Specifications and declarations that come with a callee's contract do not count.
- `P`'s `deps.std` must be exactly the standard library units that `P`'s units reach: the ones they use directly, and those that these reach in turn inside the standard library (`json` reaches `num` and `utf8`). The standard library units reached only through another package are not declared by `P`; that package declares them.

Each mismatch is a failure of `gotgan verify` (exit code 1), named on standard error. A call to a package outside the closure is an unknown name for the compiler, so `check`, `verify`, `test` and `build` fail with its diagnostic (E0304) and gotgan's hint.

**Limits.** Paths only: no registry, no fetching, no versions or version selection (the `version` of a dependency is not read). The unit namespace is flat. A package with `bin` has one executable and cannot be depended on. Directories are compared lexically.

## Building and checking gotgan

gotgan is a package (`package.json` here, units in `src/`, the executable `gotgan` with the entry unit `main`). To build it the first time:

```
bmb build src/*.bmb -o gotgan
```

Building needs `clang` and `llc`, and proving needs `z3`. After that, `gotgan check`, `gotgan verify` and `gotgan build` work on it like on any package. `scripts/check.sh` builds gotgan with `$BMB build`, runs `check`, `verify` and `build` on it (the executable `gotgan build` makes must be the same bytes), `check`, `verify` and `test` on `tests/fixtures/hex`, the dependency fixtures of `tests/fixtures/deps`, every failing fixture with its expected exit code, and `gotgan new` followed by `check`, `verify` and `test` of the new package:

```
BMB=/path/to/bmb scripts/check.sh
```

## Licence

MIT (see [LICENSE](LICENSE)). `tests/fixtures/hex` is a copy of the `hex` package of `bmb-packages`, a port of the Rust crate `hex`; its `ORIGIN.md` and licence files say where it comes from.
