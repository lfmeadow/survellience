# Collector Monitoring and Alerting

## Overview

The surveillance collector is a critical long-running process that must be monitored to ensure continuous data collection. This document describes how to monitor the collector and receive alerts when it stops running.

## Quick Start

### Simple One-Time Check

```bash
./monitor_collector.sh --check
```

Exit code `0` = running, `1` = not running.

### Continuous Monitoring (Daemon Mode)

```bash
# Run in foreground
./monitor_collector.sh --daemon

# Run in background
nohup ./monitor_collector.sh --daemon > /dev/null 2>&1 &
```

## Monitoring Options

### 1. Manual Monitoring Script

The `monitor_collector.sh` script provides comprehensive monitoring:

**Features:**
- ✅ Process health checking
- ✅ State tracking (detects transitions: running → down, down → running)
- ✅ Alert cooldown (prevents alert spam)
- ✅ Multiple alert mechanisms (email, custom commands, syslog)
- ✅ Data activity monitoring (checks for recent Parquet files)
- ✅ Recovery notifications (alerts when collector recovers)

**Usage:**
```bash
# Check once
./monitor_collector.sh --check

# Run continuously (check every 60 seconds)
./monitor_collector.sh --daemon

# With email alerts
ALERT_EMAIL=admin@example.com ./monitor_collector.sh --daemon

# With custom webhook
ALERT_COMMAND='curl -X POST https://hooks.example.com/alert -d @-' \
  ./monitor_collector.sh --daemon
```

**Configuration:**
- Check interval: 60 seconds (default)
- Alert cooldown: 300 seconds (5 minutes)
- Log file: `monitor.log`
- Alert log: `alerts.log`

### 2. Systemd Monitoring (Recommended for Production)

If using systemd to manage the collector, systemd provides built-in monitoring:

**Service File** (`/etc/systemd/system/surveillance-collect.service`):
```ini
[Unit]
Description=Surveillance Data Collector
After=network.target

[Service]
Type=simple
User=your-user
WorkingDirectory=/path/to/survellience
ExecStart=/path/to/survellience/target/release/surveillance_collect /path/to/survellience/config/surveillance.toml
Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

**Systemd Features:**
- ✅ Automatic restart on failure
- ✅ Process monitoring via systemd
- ✅ Logs via `journalctl`
- ✅ Status checking: `systemctl status surveillance-collect`
- ✅ Alerts via systemd service monitoring tools (e.g., Nagios, Prometheus)

**Monitoring Commands:**
```bash
# Check status
sudo systemctl status surveillance-collect

# Watch logs
sudo journalctl -u surveillance-collect -f

# Check if running
sudo systemctl is-active surveillance-collect

# Check for failures
sudo systemctl is-failed surveillance-collect
```

**Systemd Notifications:**
- Set up systemd email notifications via `OnFailure` directive
- Use systemd timers to check service health
- Integrate with monitoring tools (Prometheus, Grafana, etc.)

### 3. Cron-Based Monitoring

Use cron to run periodic health checks:

**Add to crontab** (`crontab -e`):
```bash
# Check every 5 minutes, alert if down
*/5 * * * * /path/to/survellience/monitor_collector.sh --check || /path/to/survellience/alert_if_down.sh
```

**Simple alert script** (`alert_if_down.sh`):
```bash
#!/bin/bash
# alert_if_down.sh
if ! pgrep -f surveillance_collect > /dev/null; then
    echo "Collector is DOWN at $(date)" | mail -s "Alert: Collector Down" admin@example.com
    # Or send to webhook, Slack, etc.
fi
```

### 4. Process Managers (Supervisor, Monit, etc.)

**Supervisor** (`/etc/supervisor/conf.d/surveillance-collect.conf`):
```ini
[program:surveillance-collect]
command=/path/to/survellience/target/release/surveillance_collect /path/to/survellience/config/surveillance.toml
directory=/path/to/survellience
user=your-user
autostart=true
autorestart=true
redirect_stderr=true
stdout_logfile=/path/to/survellience/collector.log
```

**Monit** (`/etc/monit/conf.d/surveillance-collect`):
```
check process surveillance_collect
    with pidfile /var/run/surveillance-collect.pid
    start program = "/path/to/survellience/target/release/surveillance_collect /path/to/survellience/config/surveillance.toml"
    stop program = "/usr/bin/pkill -f surveillance_collect"
    if not exist then restart
    if failed url http://localhost:8080/health then alert  # if health endpoint exists
```

## Alert Mechanisms

### Email Alerts

**Using monitor_collector.sh:**
```bash
ALERT_EMAIL=admin@example.com ./monitor_collector.sh --daemon
```

**Requirements:**
- `mail` command must be installed and configured
- Email server must be accessible

**Install mail on Ubuntu/Debian:**
```bash
sudo apt-get install mailutils
```

**Using systemd OnFailure:**
```ini
[Service]
OnFailure=status-email-user@%n.service
```

### Webhook Alerts

**Slack Webhook:**
```bash
ALERT_COMMAND='curl -X POST -H "Content-Type: application/json" \
  -d "{\"text\":\"🚨 Surveillance Collector is DOWN: $(cat)\"}" \
  https://hooks.slack.com/services/YOUR/WEBHOOK/URL' \
  ./monitor_collector.sh --daemon
```

**Discord Webhook:**
```bash
ALERT_COMMAND='curl -X POST -H "Content-Type: application/json" \
  -d "{\"content\":\"🚨 Surveillance Collector is DOWN: $(cat)\"}" \
  https://discord.com/api/webhooks/YOUR/WEBHOOK/URL' \
  ./monitor_collector.sh --daemon
```

**Generic HTTP Webhook:**
```bash
ALERT_COMMAND='curl -X POST https://api.example.com/alerts -d @-' \
  ./monitor_collector.sh --daemon
```

### Syslog Alerts

The monitoring script automatically logs alerts to syslog (if `logger` is available):

```bash
# View alerts
sudo journalctl -t surveillance-monitor -f

# Or on systems without systemd
tail -f /var/log/syslog | grep surveillance-monitor
```

### Custom Alert Scripts

Create a custom alert script and call it:

**Custom alert script** (`custom_alert.sh`):
```bash
#!/bin/bash
# custom_alert.sh
message=$(cat)

# Send to multiple channels
echo "$message" | mail -s "Alert" admin@example.com
curl -X POST https://hooks.slack.com/services/YOUR/WEBHOOK -d "{\"text\":\"$message\"}"
# Add other notification channels...
```

**Usage:**
```bash
ALERT_COMMAND='./custom_alert.sh' ./monitor_collector.sh --daemon
```

## Health Check Integration

### Simple Health Check Script

The existing `health_check.sh` can be used for quick checks:

```bash
./health_check.sh
```

### Integration with Monitoring Systems

**Prometheus Node Exporter:**
- Use a custom exporter that checks collector process
- Expose metrics for Prometheus scraping

**Nagios/Icinga:**
- Create a plugin that checks collector process
- Configure alerts in monitoring system

**Example Nagios Plugin:**
```bash
#!/bin/bash
# check_surveillance_collector.sh
if pgrep -f surveillance_collect > /dev/null; then
    echo "OK - Collector is running"
    exit 0
else
    echo "CRITICAL - Collector is not running"
    exit 2
fi
```

## Data Activity Monitoring

In addition to process monitoring, monitor data activity:

```bash
# Check for recent files
find data/orderbook_snapshots -name "*.parquet" -mmin -10 | wc -l

# Alert if no recent files (even if process is running)
if [ $(find data/orderbook_snapshots -name "*.parquet" -mmin -10 | wc -l) -eq 0 ]; then
    echo "WARNING: No recent data files (process may be stuck)"
fi
```

The `monitor_collector.sh` script includes data activity checks.

## Recommended Setup

### For Development/Testing

Use the monitoring script in daemon mode:

```bash
nohup ./monitor_collector.sh --daemon > /dev/null 2>&1 &
```

### For Production

**Option 1: Systemd (Recommended)**
- Use systemd to manage the collector
- Set `Restart=on-failure` for automatic recovery
- Use systemd monitoring tools for alerts

**Option 2: Systemd + Monitoring Script**
- Use systemd to manage the collector
- Run monitoring script as a separate service
- Get alerts from monitoring script

**Option 3: Process Manager**
- Use Supervisor or Monit
- Built-in process monitoring and restart
- Configure alerts in process manager

## Troubleshooting

### Collector Stops Frequently

**Check logs:**
```bash
tail -f collector.log
# Or with systemd
journalctl -u surveillance-collect -f
```

**Common causes:**
- WebSocket connection issues
- Memory leaks
- Network problems
- Configuration errors

### Alerts Not Working

**Test email:**
```bash
echo "Test" | mail -s "Test" your-email@example.com
```

**Test webhook:**
```bash
curl -X POST https://your-webhook-url -d "test message"
```

**Check logs:**
```bash
tail -f monitor.log
tail -f alerts.log
```

### False Positives

- Adjust alert cooldown period
- Check if process name matches exactly
- Verify collector binary path

## Monitoring Checklist

- [ ] Process monitoring configured
- [ ] Alert mechanism configured (email/webhook/custom)
- [ ] Alerts tested (stop collector, verify alert received)
- [ ] Recovery notifications tested (restart collector, verify notification)
- [ ] Data activity monitoring configured
- [ ] Logs accessible and monitored
- [ ] Automated restart configured (systemd/process manager)
- [ ] Monitoring runs continuously (daemon/systemd/cron)

## Files Reference

- `monitor_collector.sh` - Main monitoring script
- `health_check.sh` - Quick health check script
- `monitor.log` - Monitoring activity log
- `alerts.log` - Alert log (all alerts)
- `.monitor_state` - State tracking file
- `.last_alert` - Last alert timestamp (for cooldown)
