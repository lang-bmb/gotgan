#!/usr/bin/env bash
# Runs the tests of the hex package: builds tests/driver.bmb together with
# src/hex.bmb (the build proves every obligation first), then runs each case of
# tests/cases.txt and tests/reference.txt through the executable and compares
# its standard output with the expected bytes. Exits non-zero on any mismatch.
#
#     BMB=/path/to/bmb tests/run.sh
set -u

here=$(cd "$(dirname "$0")" && pwd)
pkg=$(dirname "$here")
bmb=${BMB:-bmb}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if ! "$bmb" build "$pkg/src/hex.bmb" "$here/driver.bmb" -o "$work/driver" > "$work/build.log" 2>&1; then
    grep -v '"status":"verified"' "$work/build.log" >&2
    echo "hex: the test driver does not build" >&2
    exit 1
fi

pass=0
fail=0
for file in cases.txt reference.txt; do
    n=0
    while IFS= read -r line || [ -n "$line" ]; do
        n=$((n + 1))
        case "$line" in '' | '#'*) continue ;; esac
        op=${line%%|*}
        rest=${line#*|}
        input=${rest%%|*}
        expected=${rest#*|}
        printf '%b' "$input" > "$work/in"
        printf '%b' "$expected" > "$work/expected"
        read -r -a words <<< "$op"
        if "$work/driver" "${words[@]}" < "$work/in" > "$work/out" 2> "$work/err" \
            && cmp -s "$work/out" "$work/expected"; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
            echo "FAIL tests/$file:$n: $op" >&2
            echo "  expected: $(od -An -c "$work/expected" | head -3 | tr -s ' ')" >&2
            echo "  got:      $(od -An -c "$work/out" | head -3 | tr -s ' ')" >&2
            cat "$work/err" >&2
        fi
    done < "$here/$file"
done

echo "hex: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
