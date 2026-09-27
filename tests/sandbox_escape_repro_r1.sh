#!/usr/bin/env bash
# The sandbox test is meant to show that `tk` run through the shared helper never
# reaches a default database location. This script breaks the helper in a scratch
# copy of the package so that commands no longer run inside the sandbox environment,
# and expects the sandbox test to notice. It exits non-zero when the sandbox test
# still passes against a broken helper.
#
# Usage: bash tests/sandbox_escape_repro_r1.sh   (from the package directory)
set -euo pipefail

pkg="$(cd "$(dirname "$0")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
export CARGO_TARGET_DIR="$scratch/target"

helper_line='cmd.env_clear().envs(vars).current_dir(self.work());'
test_name='helper_without_taskist_db_still_stays_inside_the_sandbox'

run_sandbox_test() {
    local dir="$1"
    local rc=0
    (cd "$dir" && cargo nextest run --locked --no-fail-fast --test sandbox -E "test($test_name)" >"$dir.log" 2>&1) || rc=$?
    # Prove the named test is the one that ran, whatever the outcome.
    grep -qE "PASS|FAIL" "$dir.log" && grep -qF "$test_name" "$dir.log" || {
        cat "$dir.log"
        echo "the sandbox test did not run in $dir"
        exit 3
    }
    grep -E "tests? run:" "$dir.log" | sed "s|^|  $(basename "$dir"): |"
    return "$rc"
}

prepare() {
    local dir="$scratch/$1"
    mkdir -p "$dir"
    (cd "$pkg" && git ls-files -z | xargs -0 cp --parents -t "$dir")
    grep -qF "$helper_line" "$dir/tests/common/mod.rs" || {
        echo "helper line not found; cannot mutate"
        exit 3
    }
    echo "$dir"
}

# Control: the unmodified helper passes.
control="$(prepare control)"
if ! run_sandbox_test "$control"; then
    cat "$control.log"
    echo "control failed: the unmodified sandbox test does not pass"
    exit 3
fi
echo "control: unmodified sandbox test passes"

status=0
# Mutation A: the helper points HOME outside the temporary directory and sets neither
# TASKIST_DB nor XDG_DATA_HOME, so tk would resolve /nonexistent-outside/.local/share/...
# Mutation B: the helper inherits the whole caller environment (no env_clear, no vars).
declare -A mutations=(
    [outside_home]='let _ = vars; cmd.env_clear().env("HOME", "/nonexistent-outside").current_dir(self.work());'
    [inherited_env]='let _ = vars; cmd.current_dir(self.work());'
)
for name in outside_home inherited_env; do
    dir="$(prepare "$name")"
    replacement="${mutations[$name]}"
    HELPER_LINE="$helper_line" REPLACEMENT="$replacement" \
        perl -0pi -e 's/\Q$ENV{HELPER_LINE}\E/$ENV{REPLACEMENT}/' "$dir/tests/common/mod.rs"
    grep -qF "$replacement" "$dir/tests/common/mod.rs" || {
        echo "$name: mutation not applied"
        exit 3
    }
    if run_sandbox_test "$dir"; then
        echo "FAIL $name: sandbox test passes although the helper no longer confines tk to the temporary directory"
        status=1
    else
        echo "ok $name: sandbox test detects the broken helper"
    fi
done
exit "$status"
