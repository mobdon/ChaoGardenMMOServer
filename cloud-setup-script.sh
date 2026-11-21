#!/bin/bash
# Sonic MMO tool kit: paste into the cloud environment's Setup script. Installs before Claude starts so plugins and MCP
# servers load in the first session. The kit comes from the `toolkit` branch of ChaoGardenMMOServer, so it follows any
# account that can reach the repo; the project's shared folder is the fallback. Never fails (a failing Setup script
# blocks sessions). Log: /tmp/toolkit-setup.log
export PATH="$HOME/.local/bin:/opt/claude-code/bin:/opt/node22/bin:$PATH"
repo="$PWD/ChaoGardenMMOServer"
export KIT_DIR="$HOME/claude-toolkit" KIT_KEYS_IN_ENV=1
[ -d "$repo/.git" ] && export REPO_DIR="$repo"
# persistent data (task-observer log, claude-mem save, graphify graphs): the shared folder when mounted, else local
if [ -d /mnt/project-files ]; then export SHARED_DIR=/mnt/project-files/toolkit-data; else export SHARED_DIR="$HOME/toolkit-data"; fi
{
  rm -rf "$KIT_DIR" && mkdir -p "$KIT_DIR"
  if [ -d "$repo/.git" ] && git -C "$repo" fetch -q --depth 1 origin toolkit && git -C "$repo" archive FETCH_HEAD | tar -x -C "$KIT_DIR"; then
    echo "kit: toolkit branch $(git -C "$repo" rev-parse --short FETCH_HEAD)"
  elif [ -f /mnt/project-files/toolkit/claude-toolkit/install-all.sh ]; then
    export KIT_DIR=/mnt/project-files/toolkit/claude-toolkit; echo "kit: shared folder fallback"
  fi
  if [ -f "$KIT_DIR/install-all.sh" ]; then bash "$KIT_DIR/install-all.sh"; else echo "kit not found (no toolkit branch, no shared folder)"; fi
} > /tmp/toolkit-setup.log 2>&1
exit 0
