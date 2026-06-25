#!/bin/sh
# Intelligent VPN Rotator v2
# Proactive, rate-limit-aware VPN rotation with IP verification.
#
# Key improvements over v1:
#   - Fast rotation on rate-limit signals (30s cooldown, not 30min)
#   - IP verification after rotation (retries if same IP assigned)
#   - Rate-limit tracking with sliding window (proactive rotation)
#   - Exponential backoff on repeated failures
#   - Adaptive signal checking (faster when rate-limited)
#
# Signal types from gateway:
#   - 429_rate_limit: SearXNG got HTTP 429
#   - zero_results_after_retry: both attempts returned 0 results
#   - request_failed: SearXNG request completely failed

SIGNAL_FILE="/tmp/vpn-signals/rotate_signal"
HEALTH_URL="http://127.0.0.1:8000/v1/publicip/ip"
CONTROL_URL="http://127.0.0.1:8000/v1/vpn/status"

# ── Timing Parameters ──
MIN_INTERVAL_RATELIMIT=30     # 30s cooldown for rate-limit rotations
MIN_INTERVAL_HEALTHFAIL=60    # 60s cooldown for health failures
MIN_INTERVAL_FORCED=7200      # 2h minimum for forced rotation
MAX_INTERVAL=14400            # 4h maximum (forced rotation)
HEALTH_CHECK_INTERVAL=120     # Check health every 2 minutes
SIGNAL_CHECK_FAST=5           # Check signal every 5s when rate-limited
SIGNAL_CHECK_NORMAL=15        # Normal signal check interval
CONSECUTIVE_FAIL_THRESHOLD=5  # Health failures before rotation (let gluetun self-heal first)

# ── Rate-Limit Tracking ──
RATELIMIT_WINDOW=300          # 5-minute sliding window
RATELIMIT_THRESHOLD=3         # Proactive rotation after N rate-limits in window
MAX_VERIFY_RETRIES=3          # Max attempts to get a new IP
BACKOFF_MULTIPLIER=2          # Exponential backoff multiplier
MAX_BACKOFF=300               # Max backoff 5 minutes

mkdir -p /tmp/vpn-signals

last_rotation=0
consecutive_fails=0
last_health_check=0
current_backoff=0
rate_limit_count=0
last_rate_limit_time=0
signal_mode="normal"  # "normal" or "rate_limited"

# Rate-limit timestamps (sliding window)
ratelimit_timestamps=""

echo "$(date -u) [vpn-rotator] Intelligent rotator v2 started"
echo "$(date -u) [vpn-rotator] Rate-limit cooldown: ${MIN_INTERVAL_RATELIMIT}s, Forced: ${MIN_INTERVAL_FORCED}s"

get_current_ip() {
    local resp=$(wget -qO- --timeout=10 "http://127.0.0.1:8000/v1/publicip/ip" 2>/dev/null)
    # Extract IP from JSON: {"public_ip":"1.2.3.4",...}
    echo "$resp" | sed -n 's/.*"public_ip"\s*:\s*"\([^"]*\)".*/\1/p'
}

# PUT request via nc (netcat) — busybox wget doesn't support PUT
gluetun_put() {
    local url="$1"
    local data="$2"
    local host=$(echo "$url" | sed -E 's|https?://([^/:]+).*|\1|')
    local port=$(echo "$url" | sed -E 's|https?://[^:]+:([0-9]+).*|\1|')
    local path=$(echo "$url" | sed -E 's|https?://[^/]+(/.*)|\1|')
    [ -z "$path" ] && path="/"
    printf "PUT %s HTTP/1.0\r\nHost: %s\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n%s"         "$path" "$host" "${#data}" "$data" | nc -w 10 "$host" "$port" > /dev/null 2>&1
}

# Count rate-limit events in the sliding window
count_recent_ratelimits() {
    local now=$(date +%s)
    local cutoff=$((now - RATELIMIT_WINDOW))
    local count=0
    for ts in $ratelimit_timestamps; do
        if [ "$ts" -ge "$cutoff" ] 2>/dev/null; then
            count=$((count + 1))
        fi
    done
    echo "$count"
}

# Add a rate-limit timestamp to the sliding window
record_ratelimit() {
    local now=$(date +%s)
    local cutoff=$((now - RATELIMIT_WINDOW))
    # Prune old timestamps
    local new_timestamps=""
    for ts in $ratelimit_timestamps; do
        if [ "$ts" -ge "$cutoff" ] 2>/dev/null; then
            new_timestamps="$new_timestamps $ts"
        fi
    done
    ratelimit_timestamps="$new_timestamps $now"
    rate_limit_count=$(count_recent_ratelimits)
}

# Get the appropriate minimum interval based on reason
get_min_interval() {
    local reason="$1"
    case "$reason" in
        429_rate_limit*)
            echo "$MIN_INTERVAL_RATELIMIT"
            ;;
        health_check_failed*)
            echo "$MIN_INTERVAL_HEALTHFAIL"
            ;;
        periodic_10min_rotation*)
            echo "500"
            ;;
        *)
            echo "$MIN_INTERVAL_FORCED"
            ;;
    esac
}

rotate_vpn() {
    local reason="$1"
    local now=$(date +%s)
    local elapsed=$((now - last_rotation))
    local min_interval=$(get_min_interval "$reason")
    
    if [ "$elapsed" -lt "$min_interval" ]; then
        echo "$(date -u) [vpn-rotator] Rotation requested ($reason) but cooldown not reached (${elapsed}s/${min_interval}s). Queuing."
        return 1
    fi
    
    local old_ip=$(get_current_ip)
    echo "$(date -u) [vpn-rotator] Rotating VPN ($reason). Old IP: $old_ip"
    
    # Apply exponential backoff if we've been failing
    if [ "$current_backoff" -gt 0 ]; then
        echo "$(date -u) [vpn-rotator] Backoff: waiting ${current_backoff}s before rotation"
        sleep "$current_backoff"
    fi
    
    # Stop VPN
    gluetun_put "$CONTROL_URL" '{"status":"stopped"}'
    sleep 5
    
    # Start VPN
    gluetun_put "$CONTROL_URL" '{"status":"running"}'
    sleep 10
    
    # Verify IP actually changed
    local verify_attempts=0
    local new_ip=""
    while [ "$verify_attempts" -lt "$MAX_VERIFY_RETRIES" ]; do
        new_ip=$(get_current_ip)
        if [ -n "$new_ip" ] && [ "$new_ip" != "unknown" ] && [ "$new_ip" != "$old_ip" ]; then
            echo "$(date -u) [vpn-rotator] VPN rotated successfully. New IP: $new_ip"
            # Reset backoff on success
            current_backoff=0
            rm -f "$SIGNAL_FILE"
            last_rotation=$now
            consecutive_fails=0
            return 0
        fi
        verify_attempts=$((verify_attempts + 1))
        echo "$(date -u) [vpn-rotator] IP verification attempt $verify_attempts: got '$new_ip' (old: '$old_ip'). Retrying..."
        sleep 5
    done
    
    # IP didn't change — apply exponential backoff
    current_backoff=$((current_backoff * BACKOFF_MULTIPLIER))
    if [ "$current_backoff" -lt 15 ]; then
        current_backoff=15
    fi
    if [ "$current_backoff" -gt "$MAX_BACKOFF" ]; then
        current_backoff=$MAX_BACKOFF
    fi
    echo "$(date -u) [vpn-rotator] WARNING: IP unchanged after rotation. Backoff: ${current_backoff}s"
    
    rm -f "$SIGNAL_FILE"
    last_rotation=$now
    return 1
}

while true; do
    now=$(date +%s)
    elapsed=$((now - last_rotation))
    
    # ── 1. Check signal file (written by gateway on rate limit) ──
    if [ -f "$SIGNAL_FILE" ]; then
        reason=$(cat "$SIGNAL_FILE" 2>/dev/null || echo "unknown")
        echo "$(date -u) [vpn-rotator] Signal detected: $reason"
        rm -f "$SIGNAL_FILE"
        
        # Track rate-limit events
        if echo "$reason" | grep -q "429\|rate_limit"; then
            record_ratelimit
            signal_mode="rate_limited"
            echo "$(date -u) [vpn-rotator] Rate-limit #${rate_limit_count} in last ${RATELIMIT_WINDOW}s"
        fi
        
        rotate_vpn "$reason"
    fi
    
    # ── 2. Proactive rotation: too many rate-limits in window ──
    rate_limit_count=$(count_recent_ratelimits)
    if [ "$rate_limit_count" -ge "$RATELIMIT_THRESHOLD" ]; then
        echo "$(date -u) [vpn-rotator] PROACTIVE: ${rate_limit_count} rate-limits in ${RATELIMIT_WINDOW}s window — rotating preemptively"
        rotate_vpn "proactive_ratelimit_threshold"
        # Clear the timestamps after proactive rotation
        ratelimit_timestamps=""
        rate_limit_count=0
        signal_mode="normal"
    fi
    
    # ── 3. Periodic health check ──
    health_elapsed=$((now - last_health_check))
    if [ "$health_elapsed" -ge "$HEALTH_CHECK_INTERVAL" ]; then
        last_health_check=$now
        ip=$(get_current_ip)
        if [ -z "$ip" ] || [ "$ip" = "unknown" ]; then
            consecutive_fails=$((consecutive_fails + 1))
            echo "$(date -u) [vpn-rotator] Health check FAILED ($consecutive_fails consecutive)"
            if [ "$consecutive_fails" -ge "$CONSECUTIVE_FAIL_THRESHOLD" ]; then
                rotate_vpn "health_check_failed_${consecutive_fails}_times"
            fi
        else
            if [ "$consecutive_fails" -gt 0 ]; then
                echo "$(date -u) [vpn-rotator] Health recovered. IP: $ip"
            fi
            consecutive_fails=0
            # Clear rate-limited mode if health is good and no recent rate limits
            if [ "$rate_limit_count" -eq 0 ]; then
                signal_mode="normal"
            fi
        fi
    fi
    
    # ── 4. Forced rotation at MAX_INTERVAL ──
    if [ "$elapsed" -ge "$MAX_INTERVAL" ]; then
        rotate_vpn "max_interval_reached"
    fi
    
    # ── Adaptive sleep: faster when rate-limited ──
    if [ "$signal_mode" = "rate_limited" ]; then
        sleep "$SIGNAL_CHECK_FAST"
    elif [ "$elapsed" -ge $((MAX_INTERVAL - 600)) ]; then
        sleep "$SIGNAL_CHECK_FAST"
    else
        sleep "$SIGNAL_CHECK_NORMAL"
    fi
done
