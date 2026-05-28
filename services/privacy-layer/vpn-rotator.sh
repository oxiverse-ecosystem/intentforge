#!/bin/sh
# Intelligent VPN Rotator
# Rotates VPN IP based on signals, not fixed timers.
# Signals that trigger rotation:
#   1. Rate-limit signal file written by gateway (429/CAPTCHA/zero-results)
#   2. Health check failures (VPN connectivity lost)
#   3. Minimum 30-minute interval between rotations (prevent thrashing)
#   4. Maximum 4-hour interval (forced rotation for IP freshness)

SIGNAL_FILE="/tmp/vpn-signals/rotate_signal"
HEALTH_URL="http://127.0.0.1:8000/v1/publicip/ip"
CONTROL_URL="http://127.0.0.1:8000/v1/vpn/status"
MIN_INTERVAL=1800    # 30 minutes minimum between rotations
MAX_INTERVAL=14400   # 4 hours forced rotation
HEALTH_CHECK=300     # Check health every 5 minutes
SIGNAL_CHECK=15      # Check signal file every 15 seconds
CONSECUTIVE_FAIL_THRESHOLD=3

apk add --no-cache curl jq 2>/dev/null

mkdir -p /tmp/vpn-signals

last_rotation=$(date +%s)
consecutive_fails=0
last_health_check=0

echo "$(date -u) [vpn-rotator] Intelligent rotator started"
echo "$(date -u) [vpn-rotator] MIN_INTERVAL=${MIN_INTERVAL}s MAX_INTERVAL=${MAX_INTERVAL}s"

get_current_ip() {
    curl -s -m 10 "$HEALTH_URL" 2>/dev/null
}

rotate_vpn() {
    local reason="$1"
    local now=$(date +%s)
    local elapsed=$((now - last_rotation))
    
    if [ "$elapsed" -lt "$MIN_INTERVAL" ]; then
        echo "$(date -u) [vpn-rotator] Rotation requested ($reason) but MIN_INTERVAL not reached (${elapsed}s/${MIN_INTERVAL}s). Queuing."
        return 1
    fi
    
    local old_ip=$(get_current_ip)
    echo "$(date -u) [vpn-rotator] Rotating VPN ($reason). Old IP: $old_ip"
    
    # Stop VPN
    curl -s -m 10 -X PUT -d '{"status":"stopped"}' "$CONTROL_URL" > /dev/null 2>&1
    sleep 5
    
    # Start VPN
    curl -s -m 10 -X PUT -d '{"status":"running"}' "$CONTROL_URL" > /dev/null 2>&1
    sleep 10
    
    local new_ip=$(get_current_ip)
    echo "$(date -u) [vpn-rotator] VPN rotated. New IP: $new_ip"
    
    # Clear signal file
    rm -f "$SIGNAL_FILE"
    last_rotation=$now
    consecutive_fails=0
    return 0
}

while true; do
    now=$(date +%s)
    elapsed=$((now - last_rotation))
    
    # 1. Check signal file (written by gateway on rate limit)
    if [ -f "$SIGNAL_FILE" ]; then
        reason=$(cat "$SIGNAL_FILE" 2>/dev/null || echo "unknown")
        echo "$(date -u) [vpn-rotator] Signal detected: $reason"
        rotate_vpn "$reason"
    fi
    
    # 2. Periodic health check
    health_elapsed=$((now - last_health_check))
    if [ "$health_elapsed" -ge "$HEALTH_CHECK" ]; then
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
        fi
    fi
    
    # 3. Forced rotation at MAX_INTERVAL
    if [ "$elapsed" -ge "$MAX_INTERVAL" ]; then
        rotate_vpn "max_interval_reached"
    fi
    
    # Adaptive sleep: check signals more frequently near max interval
    if [ "$elapsed" -ge $((MAX_INTERVAL - 600)) ]; then
        sleep "$SIGNAL_CHECK"
    else
        sleep 30
    fi
done
