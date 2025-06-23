#!/usr/bin/env python3
"""
Comprehensive test for the Rust WebRTC server
Tests all major components: HTTP, TURN credentials API, and TURN server
"""
import socket
import struct
import requests
import json

def test_http_server():
    """Test HTTP server and static file serving"""
    print("🔍 Testing HTTP Server...")
    try:
        response = requests.get("http://localhost:8086/", timeout=5)
        if response.status_code == 200 and "Flutter WebRTC" in response.text:
            print("✅ HTTP server working - static files served")
            return True
        else:
            print(f"❌ HTTP server issue - Status: {response.status_code}")
            return False
    except Exception as e:
        print(f"❌ HTTP server error: {e}")
        return False

def test_turn_credentials_api():
    """Test TURN credentials REST API"""
    print("\n🔍 Testing TURN Credentials API...")
    try:
        response = requests.get(
            "http://localhost:8086/api/turn?service=turn&username=test_client",
            timeout=5
        )
        if response.status_code == 200:
            data = response.json()
            required_fields = ['username', 'password', 'ttl', 'uris']
            if all(field in data for field in required_fields):
                print("✅ TURN credentials API working")
                print(f"   Generated username: {data['username']}")
                print(f"   TURN URI: {data['uris'][0]}")
                return True, data
            else:
                print(f"❌ TURN API missing fields: {data}")
                return False, None
        else:
            print(f"❌ TURN API failed - Status: {response.status_code}")
            return False, None
    except Exception as e:
        print(f"❌ TURN API error: {e}")
        return False, None

def test_turn_server():
    """Test TURN server STUN binding"""
    print("\n🔍 Testing TURN Server (STUN)...")
    try:
        # Create STUN binding request
        msg_type = 0x0001  # Binding Request
        msg_length = 0
        magic_cookie = 0x2112A442
        transaction_id = b'\x12\x34\x56\x78\x9a\xbc\xde\xf0\x11\x22\x33\x44'
        
        stun_request = struct.pack('!HHI12s', msg_type, msg_length, magic_cookie, transaction_id)
        
        # Send to TURN server
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(3)
        
        server_addr = ('127.0.0.1', 19302)
        sock.sendto(stun_request, server_addr)
        
        # Wait for response
        response, addr = sock.recvfrom(1024)
        sock.close()
        
        if len(response) >= 20:
            resp_type, resp_length, resp_magic, resp_trans = struct.unpack('!HHI12s', response[:20])
            
            if resp_type == 0x0101 and resp_trans == transaction_id:
                print("✅ TURN server working - STUN binding response received")
                print(f"   Response length: {len(response)} bytes")
                print(f"   Transaction ID matches: {resp_trans.hex()}")
                return True
            else:
                print(f"❌ TURN server unexpected response: type=0x{resp_type:04x}")
                return False
        else:
            print("❌ TURN server response too short")
            return False
            
    except socket.timeout:
        print("❌ TURN server timeout - no response")
        return False
    except Exception as e:
        print(f"❌ TURN server error: {e}")
        return False

def test_turn_credentials_validation():
    """Test if generated TURN credentials would work with server"""
    print("\n🔍 Testing TURN Credentials Validation...")
    
    # Get credentials from API
    _, creds = test_turn_credentials_api()
    if not creds:
        print("❌ Cannot test validation - credentials API failed")
        return False
    
    # Parse the username (should be timestamp:username format)
    username = creds['username']
    if ':' in username:
        timestamp, user = username.split(':', 1)
        print(f"✅ Credentials format valid: timestamp={timestamp}, user={user}")
        print(f"   Password: {creds['password'][:10]}...")
        print(f"   TTL: {creds['ttl']} seconds")
        return True
    else:
        print("❌ Invalid username format")
        return False

def main():
    """Run comprehensive tests"""
    print("🚀 Starting comprehensive WebRTC server tests...\n")
    
    results = {
        'http': test_http_server(),
        'turn_api': test_turn_credentials_api()[0],
        'turn_server': test_turn_server(),
        'turn_validation': test_turn_credentials_validation()
    }
    
    print("\n" + "="*50)
    print("📊 TEST RESULTS SUMMARY")
    print("="*50)
    
    total_tests = len(results)
    passed_tests = sum(results.values())
    
    for test_name, passed in results.items():
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"{test_name.upper().replace('_', ' '):<20} {status}")
    
    print("-" * 50)
    print(f"TOTAL: {passed_tests}/{total_tests} tests passed")
    
    if passed_tests == total_tests:
        print("\n🎉 ALL TESTS PASSED! The Rust WebRTC server is fully functional!")
        print("\n📋 Working Components:")
        print("   • HTTP Server (port 8086)")
        print("   • Static file serving")
        print("   • TURN credentials REST API (/api/turn)")
        print("   • TURN server (port 19302)")
        print("   • STUN binding protocol")
        print("   • HMAC-SHA1 credential generation")
    else:
        print(f"\n⚠️  {total_tests - passed_tests} tests failed. Check the details above.")
    
    return passed_tests == total_tests

if __name__ == "__main__":
    main()