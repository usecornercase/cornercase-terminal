#!/bin/sh
set -eu

fail() {
    echo "::error::$1"
    exit 1
}

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
latest=$(git tag --list 'v*' --sort=-v:refname | head -n 1 | sed 's/^v//')
attempts=3

run() { if [ -n "${DRY_RUN:-}" ]; then echo "dry run: $*"; else "$@"; fi; }
app_changes() { git diff --name-only "$@" -- src Cargo.toml Cargo.lock rust-toolchain.toml .cargo; }
releases() { gh release list --exclude-drafts --limit 100 --json tagName --jq '.[].tagName'; }
listed() { printf '%s\n' "$2" | grep -qx "$1"; }
dispatch() { run gh workflow run release.yml --ref "$1" -f tag="$1"; }
runs() { gh run list --workflow release.yml --branch "$1" --json databaseId,status,attempt,url,conclusion --jq '.[] | "\(.databaseId) \(.status) \(.attempt) \(.url) \(.conclusion)"'; }
ongoing() { printf '%s\n' "$1" | awk 'NF && $2 != "completed" { found = 1 } END { exit !found }'; }
broken_jobs() { gh run view "$1" --attempt "$2" --json jobs --jq '.jobs[] | select(.conclusion != "success" and .conclusion != "skipped" and .conclusion != "neutral") | "\(.steps | length) \(.name)"'; }
newest_release() { printf '%s\n' $1 | sed 's/^v//' | sort -V | tail -n 1; }
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
        list=$(runs "$tag")
        if ongoing "$list"; then
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
    body=$(gh release view "$tag" --json body --jq '.body | gsub("\r"; "")')
    missing=""
    for v in $versions; do
        if ! printf '%s\n' "$body" | grep -qxF "### $v"; then missing="$missing $v"; fi
    done
    if [ -z "$missing" ]; then
        echo "the notes of $tag already hold every version since the last release"
        return
    fi
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
    if [ "$(newest_release "$released")" = "$current" ]; then
        run gh release edit "$tag" --notes-file "$file"
    else
        run gh release edit "$tag" --notes-file "$file" --latest=false
    fi
    rm -f "$file"
}

failed() {
    tag=$1
    url=$2
    attempt=${3:-1}
    why=${4:-}
    if ! is_tag "$tag"; then
        echo "$tag is not a release tag"
        return
    fi
    title="Release $tag failed"
    id=${url##*/}
    link="$url/attempts/$attempt"
    released=$(releases)
    if listed "$tag" "$released"; then
        what="The [Release run]($link) for \`$tag\` failed after its GitHub Release was published."
        todo="Re-run its failed jobs: \`gh run rerun $id --failed\`."
    else
        what="The [Release run]($link) for \`$tag\` failed, so \`$tag\` has no GitHub Release."
        todo="While \`$tag\` is the newest version, re-run its failed jobs (\`gh run rerun $id --failed\`) or run the Version workflow (\`gh workflow run version.yml\`); the next push to \`main\` does the same. Once a later version is released, leave it: re-running \`$tag\` then would make it the Latest release, and the later release's notes already carry its section."
    fi
    body="$what${why:+ $why} $todo"
    found=$(gh issue list --state all --search "\"$title\" in:title" --json number,title,state --jq "map(select(.title == \"$title\")) | .[] | \"\(.number) \(.state)\"")
    for n in $(printf '%s\n' "$found" | awk 'NF { print $1 }'); do
        text=$(gh issue view "$n" --json body,comments --jq '.body, .comments[].body')
        if printf '%s\n' "$text" | grep -qF "$link"; then
            echo "the failure of $tag's Release run (attempt $attempt) is already told in #$n"
            return
        fi
    done
    open=$(printf '%s\n' "$found" | awk '$2 == "OPEN" { print $1; exit }')
    if [ -n "$open" ]; then
        run gh issue comment "$open" --body "$body"
    else
        run gh issue create --title "$title" --body "$body"
    fi
}

settle() {
    newest=$1
    list=$(runs "$newest")
    if [ -z "$list" ]; then
        echo "$newest has no Release run"
        return
    fi
    if ongoing "$list"; then
        echo "$newest's Release run has not ended"
        return
    fi
    set -- $(printf '%s\n' "$list" | head -n 1)
    run_id=$1
    run_attempt=$3
    run_url=$4
    run_conclusion=${5:-}
    case "$run_conclusion" in
        success | skipped | neutral)
            echo "$newest's last Release run ended in $run_conclusion"
            return
            ;;
    esac
    jobs=$(broken_jobs "$run_id" "$run_attempt")
    never=$(printf '%s\n' "$jobs" | awk '$1 == 0 { sub(/^0 /, ""); print }')
    started=$(printf '%s\n' "$jobs" | awk 'NF && $1 != 0')
    why=""
    if [ "$run_conclusion" = failure ] && [ -n "$never" ] && [ -z "$started" ]; then
        if [ "$run_attempt" -lt "$attempts" ]; then
            run gh run rerun "$run_id" --failed
            echo "re-running the jobs of $newest's Release run that never started (attempt $((run_attempt + 1)) of $attempts)"
            return
        fi
        names=$(printf '%s\n' "$never" | awk 'NR > 1 { printf ", " } { printf "`%s`", $0 }')
        why="Release watch stops re-running it after $run_attempt attempts in which these jobs never started (no runner took them): $names."
    fi
    failed "$newest" "$run_url" "$run_attempt" "$why"
}

watch() {
    if [ -z "$latest" ]; then
        echo "no release tag yet"
        return
    fi
    settle "v$latest"
    recent=$(git tag --list 'v*' --sort=-v:refname | head -n 5)
    for recent_tag in $recent; do
        notes "$recent_tag"
    done
}

case "${1:-}" in
    check) check "$2" ;;
    release) release ;;
    notes) notes "$2" ;;
    failed) failed "$2" "$3" "${4:-1}" ;;
    watch) watch ;;
    *) fail "usage: version.sh check <base> | version.sh release | version.sh notes <tag> | version.sh failed <tag> <run-url> [<attempt>] | version.sh watch" ;;
esac
