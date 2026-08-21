#!/bin/sh

set -eu

repository_dir=$(CDPATH= cd -P -- "$(dirname -- "$0")" && pwd)
install_root=${V4HOOK_INSTALL_ROOT:-"${HOME}/.local"}
skill_source="$repository_dir/skills/v4hook-cli"
skill_parent=${V4HOOK_SKILLS_ROOT:-"${HOME:?HOME is required}/.agents/skills"}
skill_destination="$skill_parent/v4hook-cli"
cargo_command=${V4HOOK_CARGO:-cargo}
staging_directory=

cleanup() {
    if [ -n "$staging_directory" ] && [ -d "$staging_directory" ]; then
        rm -rf -- "$staging_directory"
    fi
}

trap cleanup EXIT HUP INT TERM

if [ ! -f "$skill_source/SKILL.md" ]; then
    echo "installation source is missing $skill_source/SKILL.md" >&2
    exit 1
fi

mkdir -p -- "$skill_parent"

skill_source_physical=$(CDPATH= cd -P -- "$skill_source" && pwd)
skill_parent_physical=$(CDPATH= cd -P -- "$skill_parent" && pwd)
skill_destination_physical="$skill_parent_physical/v4hook-cli"

if [ -d "$skill_destination" ]; then
    existing_destination_physical=$(CDPATH= cd -P -- "$skill_destination" && pwd)
else
    existing_destination_physical=
fi

if [ "$skill_source_physical" = "$skill_destination_physical" ] || \
    [ "$skill_source_physical" = "$existing_destination_physical" ]; then
    echo "refusing to replace the installation source through $skill_destination" >&2
    exit 1
fi

staging_directory=$(mktemp -d "$skill_parent/.v4hook-cli.install.XXXXXX")
staged_skill="$staging_directory/v4hook-cli"
cp -R -- "$skill_source" "$staged_skill"

if [ ! -f "$staged_skill/SKILL.md" ]; then
    echo "staged skill is missing $staged_skill/SKILL.md" >&2
    exit 1
fi

if ! diff -r -- "$skill_source" "$staged_skill" >/dev/null; then
    echo "staged skill does not exactly match $skill_source" >&2
    exit 1
fi

if ! command -v "$cargo_command" >/dev/null 2>&1; then
    echo "cargo is required. Install Rust with rustup before running this script." >&2
    exit 1
fi

cd "$repository_dir"
"$cargo_command" install \
    --path . \
    --locked \
    --profile release \
    --root "$install_root" \
    --bin v4hook \
    --force

binary="$install_root/bin/v4hook"
if [ ! -x "$binary" ]; then
    echo "installation finished without creating $binary" >&2
    exit 1
fi

rm -rf -- "$skill_destination"
mv -- "$staged_skill" "$skill_destination"

if [ ! -f "$skill_destination/SKILL.md" ]; then
    echo "installation finished without creating $skill_destination/SKILL.md" >&2
    exit 1
fi

echo "Installed $("$binary" --version) at $binary"
echo "Installed v4hook-cli skill at $skill_destination"

case ":${PATH:-}:" in
    *":$install_root/bin:"*) ;;
    *) echo "Add $install_root/bin to PATH before running v4hook." >&2 ;;
esac
