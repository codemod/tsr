#!/usr/bin/env bash
cd /home/user/workspace/repo; git fetch -q origin
for b in $(git ls-remote origin 'refs/heads/box/r7-*' | awk '{print $2}' | sed 's#refs/heads/##'); do
  n=$(git rev-list --count origin/main..origin/$b 2>/dev/null)
  [ "${n:-0}" -gt 0 ] && echo "$b $n $(git merge-tree --write-tree origin/main origin/$b >/dev/null 2>&1 && echo clean || echo CONFLICT) $(git log -1 --format=%cr origin/$b)"
done
