#!/bin/sh
set -eu

fail() {
    echo "::error::$1"
    exit 1
}

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
latest=$(git tag --list 'v*' --sort=-v:refname | head -n 1 | sed 's/^v//')

run() { if [ -n "${DRY_RUN:-}" ]; then echo "dry run: $*"; else "$@"; fi; }
app_changes() { git diff --name-only "$@" -- src Cargo.toml Cargo.lock rust-toolchain.toml .cargo; }
releases() { gh release list --exclude-drafts --limit 100 --json tagName --jq '.[].tagName'; }
listed() { printf '%s\n' "$2" | grep -qx "$1"; }
dispatch() { run gh workflow run release.yml --ref "$1" -f tag="$1"; }
is_tag() { git rev-parse -q --verify "refs/tags/$1" >/dev/null; }

newer() {
    [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n 1)" = "$1" ]
}

check() {
    changed=$(app_changes "$1...HEAD")
    if [ -z "$changed" ]; then
        echo "no app changes, no new version needed"
        return
    fi
    if [ -n "$latest" ] && ! newer "$version" "$latest"; then
        fail "this pull request changes the app, so it needs a new version: set version in Cargo.toml above $latest (the last release) and add its section to CHANGELOG.md"
    fi
    grep -q "^## $version\$" CHANGELOG.md || fail "add a '## $version' section to CHANGELOG.md with what changed for users"
    echo "this pull request releases $version"
}

release() {
    tag="v$version"
    if is_tag "$tag"; then
        changed=$(app_changes "$tag" HEAD)
        [ -z "$changed" ] || fail "$tag is already tagged, but the app changed since; bump the version in a new pull request"
        released=$(releases)
        if listed "$tag" "$released"; then
            echo "$tag is already released"
            return
        fi
        pending=$(gh run list --workflow release.yml --branch "$tag" --json status --jq 'map(select(.status != "completed")) | length')
        if [ "$pending" != 0 ]; then
            echo "$tag has no release yet, and its Release run has not ended"
            return
        fi
        dispatch "$tag"
        echo "$tag has a tag but no release, so it is released again"
        return
    fi
    run git tag "$tag"
    run git push origin "$tag"
    dispatch "$tag"
    echo "releasing $tag"
}

carried() {
    git show "$1:CHANGELOG.md" | sed -n 's/^## \([0-9][0-9.]*\)$/\1/p' | while read -r v; do
        if [ "$v" = "$3" ] || newer "$3" "$v"; then
            if [ -z "$2" ] || newer "$v" "$2"; then
                echo "$v"
            fi
        fi
    done
}

previous_release() {
    best=""
    for t in $2; do
        v=${t#v}
        if newer "$1" "$v" && { [ -z "$best" ] || newer "$v" "$best"; }; then
            best=$v
        fi
    done
    echo "$best"
}

notes() {
    tag=$1
    if ! is_tag "$tag"; then
        echo "$tag is not a release tag"
        return
    fi
    released=$(releases)
    if ! listed "$tag" "$released"; then
        echo "$tag has no release, so it has no notes to complete"
        return
    fi
    current=${tag#v}
    previous=$(previous_release "$current" "$released")
    versions=$(carried "$tag" "$previous" "$current" | tr '\n' ' ')
    count=$(echo "$versions" | wc -w)
    if [ "$count" -lt 2 ]; then
        echo "the notes of $tag already hold every version since the last release"
        return
    fi
    body=$(gh release view "$tag" --json body --jq .body | tr -d '\r')
    sections=$(git show "$tag:CHANGELOG.md" | awk -v versions="$versions" '
        BEGIN { n = split(versions, list, " "); for (i = 1; i <= n; i++) wanted["## " list[i]] = 1 }
        /^## / { keep = ($0 in wanted); if (keep) $0 = "#" $0 }
        keep')
    file=$(mktemp)
    {
        printf '## Release Notes\n\n%s\n\n' "$sections"
        printf '%s\n' "$body" | awk '/^## Release Notes$/ { skip = 1; next } /^## / { skip = 0 } !skip'
    } >"$file"
    cat "$file"
    run gh release edit "$tag" --notes-file "$file"
    rm -f "$file"
}

failed() {
    tag=$1
    url=$2
    if ! is_tag "$tag"; then
        echo "$tag is not a release tag"
        return
    fi
    title="Release $tag failed"
    id=${url##*/}
    released=$(releases)
    if listed "$tag" "$released"; then
        body="The [Release run]($url) for \`$tag\` failed after its GitHub Release was published. Re-run its failed jobs: \`gh run rerun $id --failed\`."
    else
        body="The [Release run]($url) for \`$tag\` failed, so \`$tag\` has no GitHub Release. While \`$tag\` is the newest version, re-run its failed jobs (\`gh run rerun $id --failed\`) or run the Version workflow (\`gh workflow run version.yml\`); the next push to \`main\` does the same. Once a later version is released, leave it: re-running \`$tag\` then would make it the Latest release, and the later release's notes already carry its section."
    fi
    open=$(gh issue list --state open --search "\"$title\" in:title" --json number,title --jq "map(select(.title == \"$title\")) | .[0].number // empty")
    if [ -n "$open" ]; then
        run gh issue comment "$open" --body "$body"
    else
        run gh issue create --title "$title" --body "$body"
    fi
}

case "${1:-}" in
    check) check "$2" ;;
    release) release ;;
    notes) notes "$2" ;;
    failed) failed "$2" "$3" ;;
    *) fail "usage: version.sh check <base> | version.sh release | version.sh notes <tag> | version.sh failed <tag> <run-url>" ;;
esac
