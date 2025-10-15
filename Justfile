set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Internal directories for background process state
_state_dir := "tmp/devservers"
_log_dir := "tmp/devservers/logs"

# --- API service -----------------------------------------------------------
start-api:
    @mkdir -p {{_log_dir}}
    @nohup cargo run -p kavedarr-api --features dev > {{_log_dir}}/api.log 2>&1 &
    @echo $$! > {{_state_dir}}/api.pid
    @echo "API started (pid $$(cat {{_state_dir}}/api.pid))"

stop-api:
    @if [ -f {{_state_dir}}/api.pid ]; then \
        pid=$$(cat {{_state_dir}}/api.pid); \
        if kill $$pid >/dev/null 2>&1; then echo "Stopped API ($$pid)"; fi; \
        rm -f {{_state_dir}}/api.pid; \
    else \
        echo "API not running"; \
    fi

# --- Worker service -------------------------------------------------------
start-worker:
    @mkdir -p {{_log_dir}}
    @nohup cargo run -p kavedarr-downloaders --features dev > {{_log_dir}}/worker.log 2>&1 &
    @echo $$! > {{_state_dir}}/worker.pid
    @echo "Worker started (pid $$(cat {{_state_dir}}/worker.pid))"

stop-worker:
    @if [ -f {{_state_dir}}/worker.pid ]; then \
        pid=$$(cat {{_state_dir}}/worker.pid); \
        if kill $$pid >/dev/null 2>&1; then echo "Stopped worker ($$pid)"; fi; \
        rm -f {{_state_dir}}/worker.pid; \
    else \
        echo "Worker not running"; \
    fi

# --- Dashboard service ----------------------------------------------------
start-dashboard:
    @mkdir -p {{_log_dir}}
    @cd apps/dashboard && nohup npm run dev > ../../{{_log_dir}}/dashboard.log 2>&1 &
    @echo $$! > {{_state_dir}}/dashboard.pid
    @echo "Dashboard started (pid $$(cat {{_state_dir}}/dashboard.pid))"

stop-dashboard:
    @if [ -f {{_state_dir}}/dashboard.pid ]; then \
        pid=$$(cat {{_state_dir}}/dashboard.pid); \
        if kill $$pid >/dev/null 2>&1; then echo "Stopped dashboard ($$pid)"; fi; \
        rm -f {{_state_dir}}/dashboard.pid; \
    else \
        echo "Dashboard not running"; \
    fi

# --- Convenience targets --------------------------------------------------
status:
    @mkdir -p {{_state_dir}}
    @for svc in api worker dashboard; do \
        pidfile={{_state_dir}}/$$svc.pid; \
        if [ -f $$pidfile ]; then \
            pid=$$(cat $$pidfile); \
            if ps -p $$pid >/dev/null 2>&1; then \
                echo "$$svc: running (pid $$pid)"; \
            else \
                echo "$$svc: stale pid $$pid (not running)"; \
            fi; \
        else \
            echo "$$svc: not running"; \
        fi; \
    done

stop-all: stop-dashboard stop-worker stop-api
