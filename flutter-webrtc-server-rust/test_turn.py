#!/usr/bin/env python3
import socket
import struct

def create_stun_binding_request():
    """Create a STUN Binding Request message"""
    # STUN Header
    msg_type = 0x0001  # Binding Request
    msg_length = 0     # No attributes for basic request
    magic_cookie = 0x2112A442
    transaction_id = b'\x00' * 12  # 12 bytes of zeros for simplicity
    
    # Pack STUN header
    header = struct.pack('!HHI12s', msg_type, msg_length, magic_cookie, transaction_id)
    return header

def test_turn_server():
    """Test TURN server with STUN binding request"""
    try:
        # Create UDP socket
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(5)  # 5 second timeout
        
        # Create STUN binding request
        stun_request = create_stun_binding_request()
        
        # Send to TURN server
        server_addr = ('127.0.0.1', 19302)
        print(f"Sending STUN binding request to {server_addr}")
        print(f"Request data: {stun_request.hex()}")
        
        sock.sendto(stun_request, server_addr)
        
        # Wait for response
        try:
            response, addr = sock.recvfrom(1024)
            print(f"✅ Received response from {addr}")
            print(f"Response data: {response.hex()}")
            print(f"Response length: {len(response)} bytes")
            
            # Parse basic response
            if len(response) >= 20:
                msg_type, msg_length, magic, trans_id = struct.unpack('!HHI12s', response[:20])
                print(f"Response type: 0x{msg_type:04x}")
                print(f"Response length: {msg_length}")
                print(f"Magic cookie: 0x{magic:08x}")
                
                if msg_type == 0x0101:
                    print("✅ STUN Binding Success Response received!")
                else:
                    print(f"⚠️  Unexpected response type: 0x{msg_type:04x}")
            
        except socket.timeout:
            print("❌ No response received (timeout)")
            
    except Exception as e:
        print(f"❌ Error: {e}")
    finally:
        sock.close()

if __name__ == "__main__":
    test_turn_server()