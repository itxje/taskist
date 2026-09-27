#!/usr/bin/env bash
# Checks that the helper tests notice a `tk_without_db()` that inherits the caller's
# environment instead of starting from an empty one.
#
# The mutant keeps the sandbox variables (minus TASKIST_DB) and the work directory but
# drops `env_clear()`, so a TASKIST_DB set in the environment of the test run reaches
# `tk`. The script
#   1. runs the sandbox, store and cli tests on an unmodified copy (control, must pass),
#   2. runs them on the mutant with TASKIST_DB unset (expected: at least one test fails),
#   3. runs the mutant with TASKIST_DB pointing at a decoy file outside every sandbox and
#      reports whether the decoy database was created.
# Exit 0 when the mutant is caught in step 2, 1 when it survives, 2 on a setup failure.
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export PATH="$HOME/.cargo/bin:$PATH"
# One target directory per copy: cargo judges freshness by the mtimes of the files a
# build read, so a shared directory can hand one copy the binaries built from another.
target_base="${REPRO_TARGET_DIR:-$work/target}"
unset TASKIST_DB TASKIST_FORMAT TASKIST_ACTOR

copy() {
  mkdir -p "$work/$1"
  git -C "$repo" ls-files -z | (cd "$repo" && xargs -0 tar -cf -) | tar -xf - -C "$work/$1"
}

run_tests() {
  (cd "$work/$1" && CARGO_TARGET_DIR="$target_base/$1" cargo nextest run --locked --no-fail-fast --test sandbox --test store --test cli)
}

copy control
copy mutant
perl -0pi -e '
  s{pub fn tk_without_db\(&self\) -> Command \{\n        self\.program_without_db\(tk_path\(\)\)\n    \}}{pub fn tk_without_db(&self) -> Command {
        let mut cmd = std::process::Command::new(tk_path());
        cmd.envs(self.vars().into_iter().filter(|(name, _)| name != "TASKIST_DB"))
            .current_dir(self.work());
        let mut cmd = Command::from_std(cmd);
        cmd.write_stdin("");
        cmd
    }}
' "$work/mutant/tests/common/mod.rs"
if ! grep -q 'Command::new(tk_path())' "$work/mutant/tests/common/mod.rs"; then
  echo "setup: mutation not applied"
  exit 2
fi
echo "== mutant tk_without_db =="
sed -n '/pub fn tk_without_db/,/^    }/p' "$work/mutant/tests/common/mod.rs"

echo "== control =="
if ! run_tests control; then
  echo "setup: control run failed"
  exit 2
fi

echo "== mutant, TASKIST_DB unset =="
if run_tests mutant; then
  survived=1
  echo "RESULT: mutant survived, every sandbox, store and cli test passed"
else
  survived=0
  echo "RESULT: mutant caught"
fi

echo "== mutant, TASKIST_DB set to a decoy outside the sandbox =="
decoy="$work/outside/user.db"
TASKIST_DB="$decoy" run_tests mutant || true
if [ -f "$decoy" ]; then
  echo "decoy database created at a path outside every sandbox: tk_without_db reached it"
else
  echo "decoy database not created"
fi

exit "$survived"
