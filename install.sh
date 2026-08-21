#!/bin/sh

set -eu

repository_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
install_root=${V4HOOK_INSTALL_ROOT:-"${HOME}/.local"}
skill_source="$repository_dir/skills/v4hook-cli"
skill_parent="${HOME:?HOME is required}/.agents/skills"
skill_destination="$skill_parent/v4hook-cli"

if [ ! -f "$skill_source/SKILL.md" ]; then
    echo "installation source is missing $skill_source/SKILL.md" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is required. Install Rust with rustup before running this script." >&2
    exit 1
fi

cd "$repository_dir"
cargo install \
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

mkdir -p -- "$skill_parent"
rm -rf -- "$skill_destination"
cp -R -- "$skill_source" "$skill_destination"

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
