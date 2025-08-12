#!/bin/bash

echo "Scanning for Chrome instances with debug ports..."
echo "================================================"

# Find all Chrome processes with remote-debugging-port
ports=$(ps aux | grep -E "chrome.*remote-debugging-port" | grep -v "port=0" | grep -v grep | sed -n 's/.*--remote-debugging-port=\([0-9]*\).*/\1/p' | sort -u)

if [ -z "$ports" ]; then
    echo "No Chrome instances found with debug ports enabled."
    exit 1
fi

for port in $ports; do
    echo ""
    echo "Found Chrome on port: $port"
    echo "----------------------------"
    
    # Try to get Chrome info via DevTools protocol
    response=$(curl -s http://127.0.0.1:$port/json/version 2>/dev/null)
    
    if [ $? -eq 0 ] && [ ! -z "$response" ]; then
        echo "✓ Port is accessible"
        browser=$(echo "$response" | grep -o '"Browser":"[^"]*"' | cut -d'"' -f4)
        ws_url=$(echo "$response" | grep -o '"webSocketDebuggerUrl":"[^"]*"' | cut -d'"' -f4)
        
        [ ! -z "$browser" ] && echo "  Browser: $browser"
        [ ! -z "$ws_url" ] && echo "  WebSocket: $ws_url"
        
        echo ""
        echo "To connect in codex, use: /browser local $port"
    else
        echo "✗ Port is not accessible (Chrome might be restricted)"
    fi
done

echo ""
echo "================================================"
echo "Tip: To launch Chrome with a debug port manually:"
echo "/Applications/Google\\ Chrome.app/Contents/MacOS/Google\\ Chrome --remote-debugging-port=9222"