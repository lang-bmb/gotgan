#!/usr/bin/env bash
# Checks gotgan. It builds gotgan from its sources with `$BMB build` (which
# proves them first), then runs it:
#   1. on itself: check and verify pass, and `gotgan build` builds the same
#      executable byte for byte;
#   2. on the hex package (tests/fixtures/hex): check, verify and test pass;
#   3. on failing fixtures, each with its exit code: broken declarations (2),
#      an unproved obligation, an unneeded assume, an unproved termination and
#      undeclared standard library units (verify, 1), a wrong expected test
#      output (test, 1) and a case file outside the format (test, 2);
#   4. `gotgan new` in a fresh directory, then check, verify and test of the
#      package it wrote, and the names and places `new` refuses (2);
#   5. command lines it refuses (2).
# Every line gotgan writes on standard output must be a JSON object. The
# compiler is $BMB (default: `bmb` on PATH). Exits non-zero on any mismatch.
set -u

root=$(cd "$(dirname "$0")/.." && pwd)
export BMB=${BMB:-bmb}
if ! command -v "$BMB" > /dev/null 2>&1; then
    echo "check: compiler not found: $BMB (set BMB)" >&2
    exit 2
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fixtures="$root/tests/fixtures"
failed=0

# The sources in the order of their unit names (each src/<unit>.bmb holds
# `mod <unit>`): `bmb fmt --check` with several files compares them in that
# order.
mapfile -t srcs < <(ls "$root"/src/*.bmb | sort)
if ! "$BMB" build "${srcs[@]}" -o "$work/gotgan" > "$work/build.log" 2>&1; then
    grep -v '"status":"verified"' "$work/build.log" >&2
    echo "check: gotgan does not build" >&2
    exit 1
fi
gotgan="$work/gotgan"

# expect CODE PATTERN ARGS..: runs gotgan with ARGS and fails the check unless
# it exits with CODE, every line of its standard output is a JSON object, and
# its standard output or standard error holds PATTERN (an extended regular
# expression; empty for none).
expect() {
    local want=$1 pattern=$2
    shift 2
    "$gotgan" "$@" > "$work/out" 2> "$work/err"
    local got=$?
    local why=""
    if [ "$got" -ne "$want" ]; then
        why="exit code $got, expected $want"
    elif ! python3 -c '
import json, sys
for line in open(sys.argv[1]):
    if not isinstance(json.loads(line), dict):
        sys.exit(1)
' "$work/out" 2> /dev/null; then
        why="a line of standard output is not a JSON object"
    elif [ -n "$pattern" ] && ! cat "$work/out" "$work/err" | grep -q -E -e "$pattern"; then
        why="no match for /$pattern/"
    fi
    if [ -n "$why" ]; then
        echo "FAIL gotgan $*: $why" >&2
        sed 's/^/  out: /' "$work/out" >&2
        sed 's/^/  err: /' "$work/err" >&2
        failed=1
    else
        echo "ok   gotgan $*"
    fi
}

# 1. gotgan on itself.
expect 0 '"command":"check","package":"gotgan".*"ok":true' --package "$root" check
expect 0 '"command":"verify","package":"gotgan".*"unproved":0,"escapes":0,"trusted":0,"termination_unproved":0.*"ok":true' --package "$root" verify
cat "$work/out"
expect 0 '"bin":"gotgan".*"ok":true' --package "$root" build
if ! cmp -s "$gotgan" "$root/target/gotgan/gotgan"; then
    echo "FAIL gotgan build: target/gotgan/gotgan differs from the executable bmb build made" >&2
    failed=1
fi

# 2. The hex package.
expect 0 '"command":"check","package":"hex".*"ok":true' --package "$fixtures/hex" check
expect 0 '"command":"verify","package":"hex".*"std":\["num"\],"ok":true' --package "$fixtures/hex" verify
expect 0 '"command":"test","package":"hex".*"passed":800,"failed":0,"ok":true' --package "$fixtures/hex" test
cat "$work/out"

# 3. Failing fixtures.
expect 2 'only `std`' --package "$fixtures/decl/other_deps" check
expect 2 'the unit `fn` cannot name a unit' --package "$fixtures/decl/bad_unit" check
expect 2 'unknown member `dependencies`' --package "$fixtures/decl/unknown_member" verify
expect 2 'not MAJOR.MINOR.PATCH' --package "$fixtures/decl/bad_version" test
expect 2 'has the name of a unit of the package' --package "$fixtures/decl/test_unit_clash" check
expect 2 'not a JSON text' --package "$fixtures/decl/not_json" check
expect 2 'cannot read .*/package.json' --package "$fixtures/decl" check
expect 0 '"ok":true' --package "$fixtures/unproved" check
expect 1 '"code":"E0802".*' --package "$fixtures/unproved" verify
expect 1 '"unproved":1,.*"ok":false' --package "$fixtures/unproved" verify
expect 1 '"escapes":1,.*"ok":false' --package "$fixtures/escape" verify
expect 1 '"termination_unproved":1,.*"ok":false' --package "$fixtures/termination" verify
expect 1 'it reaches \[num\] and deps.std lists \[str\]' --package "$fixtures/std_mismatch" verify
expect 1 '"line":3,.*"expected":"4\\\\x0a","got":"3\\\\x0a"' --package "$fixtures/wrong_output" test
expect 1 '"passed":1,"failed":1,"ok":false' --package "$fixtures/wrong_output" test
expect 2 'tests/cases.txt:4: an escape other than' --package "$fixtures/bad_cases" test

# 4. A new package.
(cd "$work" && "$gotgan" new demo > "$work/new.out" 2>&1) || {
    echo "FAIL gotgan new demo" >&2
    cat "$work/new.out" >&2
    failed=1
}
expect 0 '"command":"check","package":"demo".*"ok":true' --package "$work/demo" check
expect 0 '"command":"verify","package":"demo".*"ok":true' --package "$work/demo" verify
expect 0 '"passed":3,"failed":0,"ok":true' --package "$work/demo" test
expect 2 'File exists' new demo --package "$work/demo"
expect 2 'cannot name a package' new fn --package "$work/x1"
expect 2 'cannot name a package' new driver --package "$work/x2"
expect 2 'cannot name a package' new Demo --package "$work/x3"

# 5. Command lines.
expect 2 'usage'
expect 2 'usage' frobnicate
expect 2 'usage' check extra
expect 2 'usage' new
expect 2 'usage' --package

if [ $failed -eq 0 ]; then
    echo "check: ok"
else
    echo "check: FAILED"
fi
exit $failed
