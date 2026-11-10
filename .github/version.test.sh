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
    echo 0 >"$FAKE/pending"
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
    "run list") cat "$FAKE/pending" ;;
    "issue list") cat "$FAKE/issues" ;;
    "workflow run" | "issue create" | "issue comment") ;;
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

run_script() {
    if ! sh "$script" "$@" >"$FAKE/out" 2>&1; then fail_case "the script exited with an error"; fi
}

run_script_failing() {
    if sh "$script" "$@" >"$FAKE/out" 2>&1; then fail_case "the script should have failed"; fi
}

tag_released() {
    git tag v0.3.0
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
echo 1 >"$FAKE/pending"
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
cat >"$FAKE/body" <<'BODY'
## Release Notes

- what 0.3.0 changed

## Install cornercase 0.3.0

### Install prebuilt binaries
BODY
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
diff "$FAKE/expected" "$FAKE/body" >"$FAKE/diff" || { cat "$FAKE/diff" >>"$FAKE/out"; fail_case "the body differs"; }
cp "$FAKE/body" "$FAKE/first"
run_script notes v0.3.0
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

new_case "a failed release opens an issue once"
tag_released
run_script failed v0.3.0 https://example.com/runs/42
expect_call "gh issue create --title Release v0.3.0 failed"
echo 7 >"$FAKE/issues"
: >"$FAKE/calls"
run_script failed v0.3.0 https://example.com/runs/42
expect_call "gh issue comment 7"
expect_no_call "issue create"

new_case "a dry run writes nothing"
DRY_RUN=1 run_script release
if git rev-parse -q --verify refs/tags/v0.3.0 >/dev/null; then fail_case "a tag was made"; fi
if git -C "$dir/origin.git" rev-parse -q --verify refs/tags/v0.3.0 >/dev/null; then fail_case "a tag was pushed"; fi
expect_no_call "workflow run"
expect_output "dry run: gh workflow run"

if [ "$failures" -ne 0 ]; then
    echo "$failures checks failed in $cases cases"
    exit 1
fi
echo "version.sh: $cases cases passed"
