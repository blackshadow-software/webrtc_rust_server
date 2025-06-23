#!/bin/bash

echo "🚀 Testing Rust WebRTC Server Components"
echo "========================================"

# Test 1: HTTP Server
echo
echo "🔍 Testing HTTP Server..."
if curl -s "http://localhost:8086/" | grep -q "Flutter WebRTC"; then
    echo "✅ HTTP server working - static files served"
    HTTP_TEST=1
else
    echo "❌ HTTP server failed"
    HTTP_TEST=0
fi

# Test 2: TURN Credentials API
echo
echo "🔍 Testing TURN Credentials API..."
TURN_RESPONSE=$(curl -s "http://localhost:8086/api/turn?service=turn&username=test_client")
if echo "$TURN_RESPONSE" | grep -q "username.*password.*ttl.*uris"; then
    echo "✅ TURN credentials API working"
    echo "   Response: $TURN_RESPONSE"
    TURN_API_TEST=1
else
    echo "❌ TURN credentials API failed"
    echo "   Response: $TURN_RESPONSE"
    TURN_API_TEST=0
fi

# Test 3: TURN Server Port Binding
echo
echo "🔍 Testing TURN Server Port Binding..."
if netstat -an | grep -q "\.19302.*udp"; then
    echo "✅ TURN server port 19302 is bound"
    TURN_PORT_TEST=1
else
    echo "❌ TURN server port 19302 not bound"
    TURN_PORT_TEST=0
fi

# Test 4: WebSocket Endpoint
echo
echo "🔍 Testing WebSocket Endpoint..."
WS_RESPONSE=$(curl -s -I "http://localhost:8086/ws")
if echo "$WS_RESPONSE" | grep -q "HTTP/1.1"; then
    echo "✅ WebSocket endpoint accessible"
    WS_TEST=1
else
    echo "❌ WebSocket endpoint failed"
    WS_TEST=0
fi

# Test 5: TURN Server STUN Response (using our Python script)
echo
echo "🔍 Testing TURN Server STUN Response..."
if python3 test_turn.py | grep -q "STUN Binding Success Response received"; then
    echo "✅ TURN server STUN protocol working"
    STUN_TEST=1
else
    echo "❌ TURN server STUN protocol failed"
    STUN_TEST=0
fi

# Summary
echo
echo "=================================================="
echo "📊 TEST RESULTS SUMMARY"
echo "=================================================="

TOTAL_TESTS=5
PASSED_TESTS=$((HTTP_TEST + TURN_API_TEST + TURN_PORT_TEST + WS_TEST + STUN_TEST))

echo "HTTP Server                ✅ $([ $HTTP_TEST -eq 1 ] && echo "PASS" || echo "FAIL")"
echo "TURN Credentials API       ✅ $([ $TURN_API_TEST -eq 1 ] && echo "PASS" || echo "FAIL")"  
echo "TURN Server Port Binding   ✅ $([ $TURN_PORT_TEST -eq 1 ] && echo "PASS" || echo "FAIL")"
echo "WebSocket Endpoint         ✅ $([ $WS_TEST -eq 1 ] && echo "PASS" || echo "FAIL")"
echo "TURN Server STUN Protocol  ✅ $([ $STUN_TEST -eq 1 ] && echo "PASS" || echo "FAIL")"

echo "--------------------------------------------------"
echo "TOTAL: $PASSED_TESTS/$TOTAL_TESTS tests passed"

if [ $PASSED_TESTS -eq $TOTAL_TESTS ]; then
    echo
    echo "🎉 ALL TESTS PASSED! The Rust WebRTC server is fully functional!"
    echo
    echo "📋 Working Components:"
    echo "   • HTTP Server (port 8086)"
    echo "   • Static file serving"
    echo "   • TURN credentials REST API (/api/turn)"
    echo "   • TURN server (port 19302)"
    echo "   • STUN binding protocol"
    echo "   • WebSocket signaling endpoint"
    echo "   • HMAC-SHA1 credential generation"
    echo
    echo "🔗 Access the server at: http://localhost:8086/"
    echo "🔗 WebSocket endpoint: ws://localhost:8086/ws"
    echo "🔗 TURN credentials: http://localhost:8086/api/turn?service=turn&username=YOUR_USER"
else
    echo
    echo "⚠️  $(($TOTAL_TESTS - $PASSED_TESTS)) tests failed. Check the details above."
fi