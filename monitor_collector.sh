#!/bin/bash
# Monitor collector process and send alerts when it stops running
#
# Usage:
#   ./monitor_collector.sh                    # Check once and exit with status code
#   ./monitor_collector.sh --daemon           # Run continuously (check every 60s)
#   ./monitor_collector.sh --check            # Check once and alert if down
#
# Exit codes:
#   0 = Collector is running
#   1 = Collector is not running
#   2 = Configuration/initialization error

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COLLECTOR_BINARY="${SCRIPT_DIR}/target/release/surveillance_collect"
CONFIG_FILE="${SCRIPT_DIR}/config/surveillance.toml"
LOG_FILE="${SCRIPT_DIR}/monitor.log"
ALERT_LOG="${SCRIPT_DIR}/alerts.log"
CHECK_INTERVAL=60  # seconds
ALERT_COOLDOWN=300 # seconds (5 minutes) - don't alert more than once per cooldown period

# Alert configuration
ALERT_EMAIL="${ALERT_EMAIL:-}"  # Set via environment variable
ALERT_COMMAND="${ALERT_COMMAND:-}"  # Set via environment variable (e.g., webhook, custom script)

# State tracking
STATE_FILE="${SCRIPT_DIR}/.monitor_state"
LAST_ALERT_FILE="${SCRIPT_DIR}/.last_alert"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_FILE"
}

log_alert() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] ALERT: $*" | tee -a "$ALERT_LOG"
}

check_collector_running() {
    # Check if collector process is running
    if pgrep -f "surveillance_collect.*config" > /dev/null 2>&1; then
        return 0
    else
        return 1
    fi
}

get_collector_pid() {
    pgrep -f "surveillance_collect.*config" | head -1 || echo ""
}

get_collector_info() {
    local pid="$1"
    if [ -z "$pid" ]; then
        echo "N/A"
        return
    fi
    
    if ps -p "$pid" > /dev/null 2>&1; then
        local mem=$(ps -p "$pid" -o rss= 2>/dev/null | awk '{printf "%.1f MB", $1/1024}')
        local cpu=$(ps -p "$pid" -o %cpu= 2>/dev/null | awk '{print $1"%"}')
        local runtime=$(ps -p "$pid" -o etime= 2>/dev/null | awk '{print $1}')
        echo "PID: $pid, Memory: $mem, CPU: $cpu, Runtime: $runtime"
    else
        echo "N/A"
    fi
}

should_alert() {
    # Check if we should send an alert (respect cooldown period)
    if [ ! -f "$LAST_ALERT_FILE" ]; then
        return 0  # No previous alert, should alert
    fi
    
    local last_alert_time=$(cat "$LAST_ALERT_FILE" 2>/dev/null || echo "0")
    local current_time=$(date +%s)
    local time_since_last_alert=$((current_time - last_alert_time))
    
    if [ "$time_since_last_alert" -ge "$ALERT_COOLDOWN" ]; then
        return 0  # Cooldown expired, should alert
    else
        return 1  # Still in cooldown, don't alert
    fi
}

record_alert() {
    echo "$(date +%s)" > "$LAST_ALERT_FILE"
}

send_alert() {
    local message="$1"
    local subject="🚨 Surveillance Collector is DOWN"
    
    log_alert "$message"
    
    # Update last alert time
    record_alert
    
    # Send email alert (if configured)
    if [ -n "$ALERT_EMAIL" ] && command -v mail > /dev/null 2>&1; then
        echo "$message" | mail -s "$subject" "$ALERT_EMAIL" 2>/dev/null || true
        log "Alert email sent to $ALERT_EMAIL"
    fi
    
    # Execute custom alert command (if configured)
    if [ -n "$ALERT_COMMAND" ]; then
        eval "$ALERT_COMMAND" <<< "$message" 2>/dev/null || true
        log "Custom alert command executed"
    fi
    
    # Also log to syslog (if available)
    if command -v logger > /dev/null 2>&1; then
        logger -t surveillance-monitor "ALERT: Collector is DOWN - $message" 2>/dev/null || true
    fi
}

send_recovery_notification() {
    local message="$1"
    
    log_alert "RECOVERY: $message"
    
    # Send email notification (if configured)
    if [ -n "$ALERT_EMAIL" ] && command -v mail > /dev/null 2>&1; then
        echo "$message" | mail -s "✅ Surveillance Collector is RUNNING" "$ALERT_EMAIL" 2>/dev/null || true
    fi
    
    # Log to syslog
    if command -v logger > /dev/null 2>&1; then
        logger -t surveillance-monitor "RECOVERY: Collector is RUNNING - $message" 2>/dev/null || true
    fi
}

check_data_activity() {
    # Check if data files are being created (additional health check)
    local recent_files=$(find "${SCRIPT_DIR}/data/orderbook_snapshots" -name "*.parquet" -type f -mmin -10 2>/dev/null | wc -l)
    echo "$recent_files"
}

run_check() {
    local mode="${1:-check}"
    local was_running=false
    
    # Load previous state
    if [ -f "$STATE_FILE" ]; then
        was_running=$(cat "$STATE_FILE")
    fi
    
    # Check current state
    local is_running=false
    if check_collector_running; then
        is_running=true
        local pid=$(get_collector_pid)
        local info=$(get_collector_info "$pid")
        local recent_files=$(check_data_activity)
        
        if [ "$mode" != "daemon" ]; then
            echo -e "${GREEN}✅ Collector is RUNNING${NC}"
            echo "   $info"
            echo "   Recent files (last 10 min): $recent_files"
        else
            log "Collector is RUNNING - $info (recent files: $recent_files)"
        fi
        
        # State transition: was down, now up (recovery)
        if [ "$was_running" = "false" ]; then
            send_recovery_notification "Collector has recovered. PID: $pid, Info: $info"
        fi
        
        echo "true" > "$STATE_FILE"
        return 0
    else
        is_running=false
        local recent_files=$(check_data_activity)
        
        if [ "$mode" != "daemon" ]; then
            echo -e "${RED}❌ Collector is NOT RUNNING${NC}"
            echo "   Recent files (last 10 min): $recent_files"
        else
            log "Collector is NOT RUNNING (recent files: $recent_files)"
        fi
        
        # State transition: was running, now down (alert)
        if [ "$was_running" = "true" ] || [ ! -f "$STATE_FILE" ]; then
            if should_alert; then
                local alert_msg="Collector process is not running. Recent files: $recent_files. Last check: $(date)"
                send_alert "$alert_msg"
            else
                local time_since=$(cat "$LAST_ALERT_FILE" 2>/dev/null || echo "0")
                local time_since_sec=$(( $(date +%s) - time_since ))
                log "Alert suppressed (cooldown: ${time_since_sec}s/${ALERT_COOLDOWN}s)"
            fi
        fi
        
        echo "false" > "$STATE_FILE"
        return 1
    fi
}

run_daemon() {
    log "Starting collector monitor daemon (check interval: ${CHECK_INTERVAL}s)"
    
    # Trap signals for clean shutdown
    trap 'log "Monitor daemon stopping..."; exit 0' INT TERM
    
    while true; do
        run_check "daemon"
        sleep "$CHECK_INTERVAL"
    done
}

show_usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Monitor the surveillance collector process and send alerts when it stops.

OPTIONS:
    --check          Check once and exit with status code (default)
    --daemon         Run continuously, checking every ${CHECK_INTERVAL} seconds
    --help           Show this help message

ENVIRONMENT VARIABLES:
    ALERT_EMAIL      Email address to send alerts to (requires 'mail' command)
    ALERT_COMMAND    Custom command to execute on alert (receives message via stdin)

EXAMPLES:
    # Check once
    $0 --check

    # Run as daemon
    $0 --daemon

    # With email alerts
    ALERT_EMAIL=admin@example.com $0 --daemon

    # With custom webhook alert
    ALERT_COMMAND='curl -X POST https://hooks.example.com/alert' $0 --daemon

    # Run in background
    nohup $0 --daemon > /dev/null 2>&1 &

EXIT CODES:
    0 = Collector is running
    1 = Collector is not running
    2 = Configuration error

FILES:
    $LOG_FILE          Monitor activity log
    $ALERT_LOG         Alert log (all alerts)
    $STATE_FILE        State tracking file
    $LAST_ALERT_FILE   Last alert timestamp (for cooldown)
EOF
}

main() {
    local mode="check"
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --check)
                mode="check"
                shift
                ;;
            --daemon)
                mode="daemon"
                shift
                ;;
            --help|-h)
                show_usage
                exit 0
                ;;
            *)
                echo "Unknown option: $1" >&2
                show_usage
                exit 2
                ;;
        esac
    done
    
    # Validate configuration
    if [ ! -f "$CONFIG_FILE" ]; then
        echo "Error: Config file not found: $CONFIG_FILE" >&2
        exit 2
    fi
    
    if [ ! -f "$COLLECTOR_BINARY" ]; then
        echo "Warning: Collector binary not found: $COLLECTOR_BINARY" >&2
        echo "  Run: cargo build --release" >&2
    fi
    
    # Create log directory if needed
    mkdir -p "$(dirname "$LOG_FILE")"
    mkdir -p "$(dirname "$ALERT_LOG")"
    
    # Run check
    if [ "$mode" = "daemon" ]; then
        run_daemon
    else
        run_check "$mode"
        exit $?
    fi
}

main "$@"
