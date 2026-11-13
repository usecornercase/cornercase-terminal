#!/bin/sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
script=${VERSION_SH:-$here/version.sh}
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.com
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.com

failures=0
cases=0

write_version() {
    printf '[package]\nname = "x"\nversion = "%s"\n' "$1" >Cargo.toml
}

new_case() {
    cases=$((cases + 1))
    name=$1
    dir="$root/case$cases"
    FAKE="$dir/fake"
    mkdir -p "$FAKE/bin" "$dir/work"
    : >"$FAKE/calls"
    : >"$FAKE/releases"
    : >"$FAKE/issues"
    : >"$FAKE/body"
    : >"$FAKE/runs"
    : >"$FAKE/jobs"
    : >"$FAKE/told"
    cat >"$FAKE/bin/gh" <<'GH'
#!/bin/sh
echo "gh $*" >>"$FAKE/calls"
case "$1 $2" in
    "release list") cat "$FAKE/releases" ;;
    "release view") if [ -e "$FAKE/view_fails" ]; then exit 1; fi; cat "$FAKE/body" ;;
    "release edit")
        while [ $# -gt 0 ]; do
            if [ "$1" = --notes-file ]; then cp "$2" "$FAKE/body"; fi
            shift
        done
        ;;
    "run list") cat "$FAKE/runs" ;;
    "run view") cat "$FAKE/jobs" ;;
    "issue list")
        case "$*" in
            *"--state all"*) cat "$FAKE/issues" ;;
            *) grep -v CLOSED "$FAKE/issues" || true ;;
        esac
        ;;
    "issue view") cat "$FAKE/told" ;;
    "workflow run" | "run rerun" | "issue create" | "issue comment") ;;
    *) exit 1 ;;
esac
GH
    chmod +x "$FAKE/bin/gh"
    git init -q --bare "$dir/origin.git"
    cd "$dir/work"
    git init -q -b main
    git remote add origin "$dir/origin.git"
    printf '# Changelog\n' >CHANGELOG.md
    for v in 0.1.0 0.2.0 0.3.0; do
        write_version "$v"
        printf '# Changelog\n\n## %s\n\n- what %s changed\n' "$v" "$v" >new.md
        tail -n +2 CHANGELOG.md >>new.md
        mv new.md CHANGELOG.md
        mkdir -p src
        echo "$v" >src/main.rs
        git add -A
        git commit -q -m "release $v"
        [ "$v" = 0.3.0 ] || git tag "v$v"
    done
    PATH="$FAKE/bin:$PATH"
    export FAKE PATH
}

fail_case() {
    failures=$((failures + 1))
    echo "FAIL: $name: $1"
    echo "--- output"
    cat "$FAKE/out" 2>/dev/null || true
    echo "--- calls"
    cat "$FAKE/calls"
}

expect_output() {
    grep -q -- "$1" "$FAKE/out" || fail_case "the output lacks: $1"
}

expect_call() {
    grep -qF -- "$1" "$FAKE/calls" || fail_case "the calls lack: $1"
}

expect_no_call() {
    if grep -qF -- "$1" "$FAKE/calls"; then fail_case "the calls hold: $1"; fi
}

expect_no_rerun() {
    if grep -q '^gh run rerun' "$FAKE/calls"; then fail_case "the calls hold a re-run"; fi
}

run_script() {
    if ! sh "$script" "$@" >"$FAKE/out" 2>&1; then fail_case "the script exited with an error"; fi
}

run_script_failing() {
    if sh "$script" "$@" >"$FAKE/out" 2>&1; then fail_case "the script should have failed"; fi
}

tag_released() {
    git tag v0.3.0
}

last_run() {
    printf '42 completed %s https://example.com/runs/42 %s\n' "$1" "$2" >"$FAKE/runs"
}

never_started() {
    echo "0 build-local-artifacts (aarch64-apple-darwin)" >"$FAKE/jobs"
}

real_failure() {
    printf '15 build-local-artifacts (x86_64-apple-darwin)\n0 build-local-artifacts (aarch64-apple-darwin)\n' >"$FAKE/jobs"
}

dist_body() {
    cat >"$FAKE/body" <<'BODY'
## Release Notes

- what 0.3.0 changed

## Install cornercase 0.3.0

### Install prebuilt binaries
BODY
}

no_writes() {
    expect_no_rerun
    expect_no_call "issue create"
    expect_no_call "issue comment"
}

new_case "check, docs only"
echo docs >README.md
git add -A
git commit -q -m docs
run_script check HEAD~1
expect_output "no app changes"

new_case "check, an app change without a new version"
git tag v0.3.0
echo more >>src/main.rs
git add -A
git commit -q -m change
run_script_failing check v0.3.0
expect_output "needs a new version"

new_case "a new version is tagged and released"
run_script release
expect_call "gh workflow run release.yml --ref v0.3.0 -f tag=v0.3.0"
git -C "$dir/origin.git" rev-parse -q --verify refs/tags/v0.3.0 >/dev/null || fail_case "v0.3.0 was not pushed"

new_case "a tag with its release is left alone"
tag_released
printf 'v0.3.0\nv0.2.0\nv0.1.0\n' >"$FAKE/releases"
run_script release
expect_output "is already released"
expect_no_call "workflow run"

new_case "a tag without a release is released again"
tag_released
printf 'v0.2.0\nv0.1.0\n' >"$FAKE/releases"
run_script release
expect_call "gh workflow run release.yml --ref v0.3.0 -f tag=v0.3.0"
if git -C "$dir/origin.git" rev-parse -q --verify refs/tags/v0.3.0 >/dev/null; then fail_case "a tag was pushed"; fi

new_case "a tag whose run has not ended waits"
tag_released
printf 'v0.2.0\nv0.1.0\n' >"$FAKE/releases"
echo "7 in_progress 1 https://example.com/runs/7 " >"$FAKE/runs"
run_script release
expect_output "has not ended"
expect_no_call "workflow run"

new_case "an older tag without a release is not released again"
tag_released
printf 'v0.3.0\nv0.1.0\n' >"$FAKE/releases"
run_script release
expect_no_call "workflow run"
expect_output "is already released"

new_case "notes carry the versions that never got a release"
tag_released
printf 'v0.3.0\nv0.1.0\n' >"$FAKE/releases"
dist_body
cat >"$FAKE/expected" <<'BODY'
## Release Notes

### 0.3.0

- what 0.3.0 changed

### 0.2.0

- what 0.2.0 changed

## Install cornercase 0.3.0

### Install prebuilt binaries
BODY
run_script notes v0.3.0
expect_call "gh release edit v0.3.0 --notes-file"
expect_no_call "--latest"
diff "$FAKE/expected" "$FAKE/body" >"$FAKE/diff" || { cat "$FAKE/diff" >>"$FAKE/out"; fail_case "the body differs"; }
cp "$FAKE/body" "$FAKE/first"
: >"$FAKE/calls"
run_script notes v0.3.0
expect_no_call "release edit"
expect_output "already hold every version"
cmp -s "$FAKE/first" "$FAKE/body" || fail_case "a second run changed the body"

new_case "notes stop when the release body cannot be read"
tag_released
printf 'v0.3.0\nv0.1.0\n' >"$FAKE/releases"
touch "$FAKE/view_fails"
run_script_failing notes v0.3.0
expect_no_call "release edit"

new_case "notes after a published previous version are left alone"
tag_released
printf 'v0.3.0\nv0.2.0\n' >"$FAKE/releases"
run_script notes v0.3.0
expect_no_call "release edit"
expect_output "already hold every version"

new_case "notes of a tag without a release are left alone"
tag_released
printf 'v0.2.0\n' >"$FAKE/releases"
run_script notes v0.3.0
expect_no_call "release edit"
expect_output "has no release"

new_case "notes of a release older than the newest one leave Latest alone"
tag_released
printf 'v0.4.0\nv0.3.0\nv0.1.0\n' >"$FAKE/releases"
dist_body
run_script notes v0.3.0
expect_call "gh release edit v0.3.0 --notes-file"
expect_call "--latest=false"

new_case "a failed release opens an issue once"
tag_released
run_script failed v0.3.0 https://example.com/runs/42
expect_call "gh issue create --title Release v0.3.0 failed"
echo "7 OPEN" >"$FAKE/issues"
: >"$FAKE/calls"
run_script failed v0.3.0 https://example.com/runs/42
expect_call "gh issue comment 7"
expect_no_call "issue create"

new_case "a failure already told is not told again"
tag_released
echo "7 OPEN" >"$FAKE/issues"
echo "see https://example.com/runs/42/attempts/1" >"$FAKE/told"
run_script failed v0.3.0 https://example.com/runs/42
expect_no_call "issue comment"
expect_no_call "issue create"
expect_output "already told"

new_case "a failure told in a closed issue is not told again"
tag_released
echo "7 CLOSED" >"$FAKE/issues"
echo "see https://example.com/runs/42/attempts/1" >"$FAKE/told"
run_script failed v0.3.0 https://example.com/runs/42
expect_no_call "issue comment"
expect_no_call "issue create"

new_case "a new failure after a closed issue opens a new issue"
tag_released
echo "7 CLOSED" >"$FAKE/issues"
run_script failed v0.3.0 https://example.com/runs/42
expect_call "issue create"
expect_no_call "issue comment 7"

new_case "watch re-runs the jobs GitHub never started"
tag_released
last_run 1 failure
never_started
run_script watch
expect_call "gh run rerun 42 --failed"
expect_no_call "issue create"
expect_no_call "issue comment"

new_case "watch re-runs them again on the second attempt"
tag_released
last_run 2 failure
never_started
run_script watch
expect_call "gh run rerun 42 --failed"

new_case "watch tells when the third attempt never started either"
tag_released
last_run 3 failure
never_started
run_script watch
expect_no_rerun
expect_call "gh issue create --title Release v0.3.0 failed"
expect_call "never started"
expect_call "runs/42/attempts/3"

new_case "watch tells a job that failed in a step and re-runs nothing"
tag_released
last_run 1 failure
real_failure
run_script watch
expect_no_rerun
expect_call "issue create"

new_case "watch re-runs nothing in a cancelled run"
tag_released
last_run 1 cancelled
never_started
run_script watch
expect_no_rerun
expect_call "issue create"

new_case "watch waits while a Release run of the newest tag has not ended"
tag_released
printf '43 in_progress 2 https://example.com/runs/43 \n42 completed 1 https://example.com/runs/42 failure\n' >"$FAKE/runs"
never_started
run_script watch
expect_output "has not ended"
no_writes

new_case "watch leaves a Release run that passed alone"
tag_released
printf 'v0.3.0\nv0.2.0\nv0.1.0\n' >"$FAKE/releases"
last_run 1 success
run_script watch
expect_output "ended in success"
no_writes
expect_no_call "release edit"

new_case "watch tells a failure after the release was published"
tag_released
printf 'v0.3.0\nv0.2.0\nv0.1.0\n' >"$FAKE/releases"
last_run 1 failure
echo "6 publish-homebrew-formula" >"$FAKE/jobs"
run_script watch
expect_call "issue create"
expect_call "after its GitHub Release was published"

new_case "a failure of a later attempt is told again"
tag_released
echo "7 OPEN" >"$FAKE/issues"
echo "see https://example.com/runs/42/attempts/1" >"$FAKE/told"
run_script failed v0.3.0 https://example.com/runs/42 2
expect_call "gh issue comment 7"
expect_call "runs/42/attempts/2"

new_case "watch tells each failure once"
tag_released
last_run 1 failure
real_failure
run_script watch
expect_call "issue create"
echo "7 OPEN" >"$FAKE/issues"
echo "see https://example.com/runs/42/attempts/1" >"$FAKE/told"
: >"$FAKE/calls"
run_script watch
no_writes
expect_output "already told"

new_case "watch looks only at the newest tag"
tag_released
last_run 1 failure
real_failure
run_script watch
expect_call "--branch v0.3.0"
expect_no_call "--branch v0.2.0"

new_case "watch says when the newest tag has no Release run"
tag_released
run_script watch
expect_output "has no Release run"
no_writes

new_case "watch completes the notes of the newest releases"
tag_released
printf 'v0.3.0\nv0.1.0\n' >"$FAKE/releases"
dist_body
last_run 1 success
run_script watch
expect_call "gh release edit v0.3.0 --notes-file"
expect_no_call "release edit v0.2.0"

new_case "watch completes the notes of an older tag among the newest"
tag_released
printf 'v0.3.0\nv0.2.0\n' >"$FAKE/releases"
dist_body
last_run 1 success
run_script watch
expect_call "gh release edit v0.2.0 --notes-file"
expect_call "--latest=false"

new_case "a dry run writes nothing"
DRY_RUN=1 run_script release
if git rev-parse -q --verify refs/tags/v0.3.0 >/dev/null; then fail_case "a tag was made"; fi
if git -C "$dir/origin.git" rev-parse -q --verify refs/tags/v0.3.0 >/dev/null; then fail_case "a tag was pushed"; fi
expect_no_call "workflow run"
expect_output "dry run: gh workflow run"

new_case "a dry run of watch re-runs nothing"
tag_released
last_run 1 failure
never_started
DRY_RUN=1 run_script watch
expect_no_rerun
expect_output "dry run: gh run rerun 42 --failed"

if [ "$failures" -ne 0 ]; then
    echo "$failures checks failed in $cases cases"
    exit 1
fi
echo "version.sh: $cases cases passed"
