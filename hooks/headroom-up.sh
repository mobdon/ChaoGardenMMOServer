#!/bin/bash
# SessionStart: restart the Headroom proxy if it died since setup (headroom_compress needs it). Silent, never blocks.
curl -s --noproxy '*' --max-time 2 http://127.0.0.1:8787/health >/dev/null && exit 0
command -v headroom >/dev/null || exit 0
(HEADROOM_BEACON=off nohup headroom proxy --host 127.0.0.1 --port 8787 --no-subscription-tracking >/tmp/headroom-proxy.log 2>&1 &)
exit 0
